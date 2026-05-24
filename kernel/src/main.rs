//! Vibe Coded OS — Kernel Entry Point
#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

use core::arch::global_asm;

#[cfg(target_arch = "aarch64")]
global_asm!(
    include_str!("arch/vector_table_aarch64.s"),
);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    include_str!("arch/syscall_x86_64.s"),
    include_str!("arch/switch_x86_64.s"),
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    include_str!("arch/switch_aarch64.s"),
);

extern crate alloc;

use core::panic::PanicInfo;

pub use common::{BootInfo, FramebufferInfo, MemoryRegion, MemoryRegionKind, Stat};

mod arch;
mod mm;
mod scheduler;
mod syscalls;
mod ipc;
mod shm;
mod compat;
mod drivers;
mod userland;
mod input;
mod wm;
mod fs;
mod net;
mod pty;
mod time;
mod audio;

#[cfg(target_arch = "x86_64")]
use arch::x86_64 as arch_impl;

#[cfg(target_arch = "aarch64")]
use arch::aarch64 as arch_impl;

/// Draw the macOS-like desktop UI
fn draw_desktop() {
    use drivers::fbcon;
    unsafe {
        let w = fbcon::fb_width();
        let h = fbcon::fb_height();

        // Background gradient (dark blue to darker blue, like macOS)
        fbcon::fill_rect_v_gradient(0, 0, w, h, 0x001a3a, 0x000a1a);

        // Top menu bar - dark translucent
        let bar_h = 28;
        fbcon::fill_rect_v_gradient(0, 0, w, bar_h, 0x1a1a2e, 0x12121e);
        // Bottom border of menu bar
        fbcon::draw_hline(0, bar_h, w, 0x333344);

        // Menu bar text
        fbcon::draw_str(8, 6, "VibeOS", 0xffffff);
        fbcon::draw_str(80, 6, "Finder", 0xffffff);
        fbcon::draw_str(150, 6, "File", 0xd0d0d0);
        fbcon::draw_str(190, 6, "Edit", 0xd0d0d0);
        fbcon::draw_str(230, 6, "View", 0xd0d0d0);
        fbcon::draw_str(275, 6, "Go", 0xd0d0d0);
        fbcon::draw_str(300, 6, "Window", 0xd0d0d0);
        fbcon::draw_str(360, 6, "Help", 0xd0d0d0);

        // Clock (right side)
        fbcon::draw_str(w - 80, 6, "04:20", 0xd0d0d0);

        // Dock area - translucent bar at bottom
        let dock_y = h - 80;
        let dock_h = 70;
        let dock_x = (w - 500) / 2;
        let dock_w = 500;

        // Dock background
        fbcon::fill_rounded_rect(dock_x, dock_y, dock_w, dock_h, 16, 0x2a2a3a80);

        // Dock separator line
        fbcon::draw_hline(dock_x + 2, dock_y + 2, dock_w - 4, 0x44445580);

        // Dock app icons (colored squares with labels)
        let icon_size = 48;
        let icons = [
            (0x007aff, "Finder"),
            (0x34c759, "Terminal"),
            (0xff9500, "Settings"),
            (0xff3b30, "Activity"),
            (0x5856d6, "Browser"),
        ];
        let icon_spacing = 12;
        let start_x = dock_x + 16;
        for (i, (color, _label)) in icons.iter().enumerate() {
            let ix = start_x + i * (icon_size + icon_spacing);
            let iy = dock_y + 8;
            // Icon shadow
            fbcon::fill_rounded_rect(ix + 2, iy + 4, icon_size, icon_size, 8, 0x00000040);
            // Icon
            fbcon::fill_rounded_rect(ix, iy, icon_size, icon_size, 8, *color);
            // Icon highlight (top half slightly lighter)
            fbcon::fill_rounded_rect(ix, iy, icon_size, icon_size / 2, 8, 0xffffff20);
        }

        // Sample window
        let win_x = 100;
        let win_y = 80;
        let win_w = 600;
        let win_h = 400;

        // Window shadow
        fbcon::draw_shadow(win_x, win_y, win_w, win_h, 8);

        // Window background
        fbcon::fill_rounded_rect(win_x, win_y, win_w, win_h, 10, 0x1e1e2e);

        // Window title bar
        let title_h = 36;
        fbcon::fill_rounded_rect(win_x, win_y, win_w, title_h, 10, 0x2a2a3a);
        // Round off the bottom corners of the title bar
        fbcon::fill_rect(win_x, win_y + title_h - 10, win_w, 10, 0x2a2a3a);

        // Window title
        fbcon::draw_str(win_x + 60, win_y + 10, "Welcome to VibeOS", 0xffffff);

        // Traffic lights (close, minimize, maximize)
        let light_y = win_y + 12;
        // Close (red)
        fbcon::fill_circle_approx(win_x + 16, light_y + 6, 6, 0xff5f57);
        // Minimize (yellow)
        fbcon::fill_circle_approx(win_x + 32, light_y + 6, 6, 0xfebc2e);
        // Maximize (green)
        fbcon::fill_circle_approx(win_x + 48, light_y + 6, 6, 0x28c840);

        // Window content area
        fbcon::draw_str(win_x + 20, win_y + 60, "Vibe Coded OS v0.1.0", 0xffffff);
        fbcon::draw_str(win_x + 20, win_y + 80, "Kernel: Rust no_std", 0xaaaaaa);
        fbcon::draw_str(win_x + 20, win_y + 100, "Architecture:", 0xaaaaaa);

        #[cfg(target_arch = "x86_64")]
        fbcon::draw_str(win_x + 180, win_y + 100, "x86_64", 0x007aff);
        #[cfg(target_arch = "aarch64")]
        fbcon::draw_str(win_x + 180, win_y + 100, "ARM64", 0x007aff);

        fbcon::draw_str(win_x + 20, win_y + 120, "Scheduler: Cooperative round-robin", 0xaaaaaa);
        fbcon::draw_str(win_x + 20, win_y + 140, "IPC: Mailbox-based message passing", 0xaaaaaa);
        fbcon::draw_str(win_x + 20, win_y + 160, "Shared Memory: Kernel-managed regions", 0xaaaaaa);
        fbcon::draw_str(win_x + 20, win_y + 180, "Framebuffer: Direct pixel access", 0xaaaaaa);

        // Separator line
        fbcon::draw_hline(win_x + 20, win_y + 210, win_w - 40, 0x333344);

        fbcon::draw_str(win_x + 20, win_y + 230, "Tasks running:", 0x888888);
        fbcon::draw_str(win_x + 20, win_y + 250, "  [running] gui_task - Desktop compositor", 0x34c759);
        fbcon::draw_str(win_x + 20, win_y + 270, "  [ready]   shell_task - Terminal", 0xaaaaaa);

        // Status bar at bottom of window
        fbcon::fill_rect(win_x, win_y + win_h - 24, win_w, 24, 0x222233);
        fbcon::draw_str(win_x + 10, win_y + win_h - 20, "UTF-8  LF  100%", 0x888888);
    }
}

/// Attempt to spawn a user-space application from an embedded ELF binary.
/// Returns the PID on success, or 0 if the ELF could not be loaded.
fn spawn_userspace_app(name: &str, elf_data: &[u8]) -> usize {
    log::info!("Attempting to spawn user-space app: {} ({} bytes)", name, elf_data.len());
    let pid = scheduler::spawn_user_from_elf(elf_data);
    if pid == 0 {
        log::warn!("Failed to load ELF for user-space app: {}", name);
    } else {
        log::info!("User-space app '{}' spawned as pid={}", name, pid);
    }
    pid
}

/// Kernel-space GUI fallback task — used when user-space WindowServer is not available
/// This provides basic desktop rendering as a fallback
extern "C" fn gui_task() -> ! {
    use drivers::{fbcon, cursor};
    use input::InputEvent;
    use wm::{hit_test, hit_test_dock_icon, HitTarget, TrafficLight, DesktopLayout};

    log::info!("gui_task: starting kernel-space desktop compositor (fallback)");

    let fb_w = unsafe { fbcon::fb_width() };
    let fb_h = unsafe { fbcon::fb_height() };

    draw_desktop();

    let dock_y = (fb_h - 80) as u16;
    let dock_x = ((fb_w - 500) / 2) as u16;
    let dock_w = 500u16;

    let mut win_x: u16 = 100;
    let mut win_y: u16 = 80;
    let mut win_w: u16 = 600;
    let mut win_h: u16 = 400;
    let win_orig_x = win_x;
    let win_orig_y = win_y;
    let win_orig_w = win_w;
    let win_orig_h = win_h;

    let mut layout = DesktopLayout {
        win_x, win_y, win_w, win_h,
        dock_y, dock_x, dock_w,
    };

    let cursor_start_x = 400u16.min(fb_w as u16 - 16);
    let cursor_start_y = 300u16.min(fb_h as u16 - 16);
    cursor::draw(cursor_start_x, cursor_start_y);

    log::info!("gui_task: desktop rendered, cursor shown, entering event loop");

    let mut window_visible = true;
    let mut window_minimized = false;
    let mut window_maximized = false;
    let mut dragging = false;
    let mut drag_offset_x = 0i32;
    let mut drag_offset_y = 0i32;
    let mut last_hovered_dock: Option<usize> = None;

    loop {
        let mut needs_redraw = false;

        while let Some(event) = input::poll() {
            match event {
                InputEvent::MouseMove { x, y, buttons: _ } => {
                    if dragging {
                        let new_x = (x as i32 + drag_offset_x).max(0).min(fb_w as i32 - 100) as u16;
                        let new_y = (y as i32 + drag_offset_y).max(0).min(fb_h as i32 - 50) as u16;
                        if new_x != win_x || new_y != win_y {
                            win_x = new_x;
                            win_y = new_y;
                            layout.win_x = win_x;
                            layout.win_y = win_y;
                            needs_redraw = true;
                        }
                    }
                    let hovered_dock = hit_test_dock_icon(x, y, &layout);
                    if hovered_dock != last_hovered_dock {
                        last_hovered_dock = hovered_dock;
                        needs_redraw = true;
                    }
                    cursor::move_cursor(x, y);
                }
                InputEvent::MouseDown { button: 0, x, y } => {
                    let target = hit_test(x, y, &layout);
                    match target {
                        HitTarget::TrafficLight(TrafficLight::Close) => {
                            if window_visible {
                                log::info!("gui_task: close window clicked");
                                draw_desktop();
                                window_visible = false;
                                window_minimized = false;
                                window_maximized = false;
                            }
                        }
                        HitTarget::TrafficLight(TrafficLight::Minimize) => {
                            if window_visible && !window_minimized {
                                log::info!("gui_task: minimize window clicked");
                                window_minimized = true;
                                window_maximized = false;
                                window_visible = false;
                                needs_redraw = true;
                            }
                        }
                        HitTarget::TrafficLight(TrafficLight::Maximize) => {
                            if window_visible {
                                if window_maximized {
                                    log::info!("gui_task: restore window");
                                    win_x = win_orig_x;
                                    win_y = win_orig_y;
                                    win_w = win_orig_w;
                                    win_h = win_orig_h;
                                    window_maximized = false;
                                } else {
                                    log::info!("gui_task: maximize window");
                                    win_x = 0;
                                    win_y = 28;
                                    win_w = fb_w as u16;
                                    win_h = (fb_h - 80 - 28) as u16;
                                    window_maximized = true;
                                    window_minimized = false;
                                }
                                layout.win_x = win_x;
                                layout.win_y = win_y;
                                layout.win_w = win_w;
                                layout.win_h = win_h;
                                window_visible = true;
                                needs_redraw = true;
                            }
                        }
                        HitTarget::TitleBar if !window_maximized => {
                            dragging = true;
                            drag_offset_x = win_x as i32 - x as i32;
                            drag_offset_y = win_y as i32 - y as i32;
                        }
                        HitTarget::DockIcon(idx) => {
                            let app_names = ["Finder", "Terminal", "Settings", "Activity", "Browser"];
                            let name = if idx < app_names.len() { app_names[idx] } else { "Unknown" };
                            log::info!("gui_task: dock icon clicked: {} (idx={})", name, idx);
                            if !window_visible {
                                window_visible = true;
                                window_minimized = false;
                                needs_redraw = true;
                            }
                        }
                        HitTarget::WindowBody => {}
                        _ => {}
                    }
                }
                InputEvent::MouseUp { .. } => {
                    dragging = false;
                }
                InputEvent::KeyPress { .. } => {
                    input::push(event);
                }
                _ => {}
            }
        }

        if needs_redraw && window_visible {
            draw_desktop_custom(win_x, win_y, win_w, win_h, last_hovered_dock);
        }

        crate::net::poll();
        scheduler::yield_cpu();
    }
}

fn draw_desktop_custom(win_x: u16, win_y: u16, win_w: u16, win_h: u16, hovered_dock: Option<usize>) {
    use drivers::fbcon;
    unsafe {
        let w = fbcon::fb_width();
        let h = fbcon::fb_height();

        fbcon::fill_rect_v_gradient(0, 0, w, h, 0x001a3a, 0x000a1a);

        let bar_h = 28;
        fbcon::fill_rect_v_gradient(0, 0, w, bar_h, 0x1a1a2e, 0x12121e);
        fbcon::draw_hline(0, bar_h, w, 0x333344);

        fbcon::draw_str(8, 6, "VibeOS", 0xffffff);
        fbcon::draw_str(80, 6, "Finder", 0xffffff);
        fbcon::draw_str(150, 6, "File", 0xd0d0d0);
        fbcon::draw_str(190, 6, "Edit", 0xd0d0d0);
        fbcon::draw_str(230, 6, "View", 0xd0d0d0);
        fbcon::draw_str(275, 6, "Go", 0xd0d0d0);
        fbcon::draw_str(300, 6, "Window", 0xd0d0d0);
        fbcon::draw_str(360, 6, "Help", 0xd0d0d0);
        fbcon::draw_str(w - 80, 6, "04:20", 0xd0d0d0);

        let dock_y = h - 80;
        let dock_h = 70;
        let dock_x = (w - 500) / 2;
        let dock_w = 500;

        fbcon::fill_rounded_rect(dock_x, dock_y, dock_w, dock_h, 16, 0x2a2a3a80);
        fbcon::draw_hline(dock_x + 2, dock_y + 2, dock_w - 4, 0x44445580);

        let icon_size = 48;
        let icons = [
            (0x007aff, "Finder"),
            (0x34c759, "Terminal"),
            (0xff9500, "Settings"),
            (0xff3b30, "Activity"),
            (0x5856d6, "Browser"),
        ];
        let icon_spacing = 12;
        let start_x = dock_x + 16;
        for (i, (color, _label)) in icons.iter().enumerate() {
            let ix = start_x + i * (icon_size + icon_spacing);
            let iy = dock_y + 8;
            let is_hovered = hovered_dock == Some(i);

            if is_hovered {
                fbcon::fill_rounded_rect(ix - 4, iy - 6, icon_size + 8, icon_size + 12, 10, 0xffffff20);
            }

            fbcon::fill_rounded_rect(ix + 2, iy + 4, icon_size, icon_size, 8, 0x00000040);
            fbcon::fill_rounded_rect(ix, iy, icon_size, icon_size, 8, *color);
            fbcon::fill_rounded_rect(ix, iy, icon_size, icon_size / 2, 8, 0xffffff20);

            if is_hovered {
                fbcon::fill_rounded_rect(ix, iy, icon_size, icon_size, 8, 0xffffff15);
            }
        }

        let wx = win_x as usize;
        let wy = win_y as usize;
        let ww = win_w as usize;
        let wh = win_h as usize;
        let title_h = 36;

        fbcon::draw_shadow(wx, wy, ww, wh, 8);
        fbcon::fill_rounded_rect(wx, wy, ww, wh, 10, 0x1e1e2e);
        fbcon::fill_rounded_rect(wx, wy, ww, title_h, 10, 0x2a2a3a);
        fbcon::fill_rect(wx, wy + title_h - 10, ww, 10, 0x2a2a3a);

        fbcon::draw_str(wx + 60, wy + 10, "Welcome to VibeOS", 0xffffff);

        let light_y = wy + 12;
        fbcon::fill_circle_approx(wx + 16, light_y + 6, 6, 0xff5f57);
        fbcon::fill_circle_approx(wx + 32, light_y + 6, 6, 0xfebc2e);
        fbcon::fill_circle_approx(wx + 48, light_y + 6, 6, 0x28c840);

        fbcon::draw_str(wx + 20, wy + 60, "Vibe Coded OS v0.1.0", 0xffffff);
        fbcon::draw_str(wx + 20, wy + 80, "Kernel: Rust no_std", 0xaaaaaa);
        fbcon::draw_str(wx + 20, wy + 100, "Architecture:", 0xaaaaaa);

        #[cfg(target_arch = "x86_64")]
        fbcon::draw_str(wx + 180, wy + 100, "x86_64", 0x007aff);
        #[cfg(target_arch = "aarch64")]
        fbcon::draw_str(wx + 180, wy + 100, "ARM64", 0x007aff);

        fbcon::draw_str(wx + 20, wy + 120, "Scheduler: Cooperative round-robin", 0xaaaaaa);
        fbcon::draw_str(wx + 20, wy + 140, "IPC: Mailbox-based message passing", 0xaaaaaa);
        fbcon::draw_str(wx + 20, wy + 160, "Shared Memory: Kernel-managed regions", 0xaaaaaa);
        fbcon::draw_str(wx + 20, wy + 180, "Framebuffer: Direct pixel access", 0xaaaaaa);

        fbcon::draw_hline(wx + 20, wy + 210, ww - 40, 0x333344);

        fbcon::draw_str(wx + 20, wy + 230, "Tasks running:", 0x888888);
        fbcon::draw_str(wx + 20, wy + 250, "  [running] gui_task - Desktop compositor", 0x34c759);
        fbcon::draw_str(wx + 20, wy + 270, "  [ready]   shell_task - Terminal", 0xaaaaaa);

        fbcon::fill_rect(wx, wy + wh - 24, ww, 24, 0x222233);
        fbcon::draw_str(wx + 10, wy + wh - 20, "UTF-8  LF  100%", 0x888888);
    }
}

/// Shell task — interactive terminal
extern "C" fn shell_task() -> ! {
    log::info!("shell_task: starting");
    userland::shell::init();
    userland::shell::Shell::run();
}

#[no_mangle]
pub extern "C" fn kernel_main(boot_info: *mut BootInfo) -> ! {
    if boot_info.is_null() {
        loop {
            #[cfg(target_arch = "x86_64")]
            unsafe { core::arch::asm!("hlt"); }
            #[cfg(target_arch = "aarch64")]
            unsafe { core::arch::asm!("wfe"); }
        }
    }
    let bi = unsafe { &*boot_info };
    let mem_map = if bi.memory_map_ptr.is_null() || bi.memory_map_len == 0 {
        &[] as &[MemoryRegion]
    } else {
        unsafe { core::slice::from_raw_parts(bi.memory_map_ptr, bi.memory_map_len) }
    };

    drivers::uart::init();
    drivers::uart_logger::init();
    log::info!("kernel_main entered");

    if !bi.framebuffer.is_null() {
        unsafe { drivers::fbcon::init(&*bi.framebuffer); }
        log::info!("framebuffer initialized: {}x{}",
            unsafe { drivers::fbcon::fb_width() },
            unsafe { drivers::fbcon::fb_height() });
    }

    mm::init(mem_map);
    arch_impl::init(unsafe { &mut *boot_info });
    input::init();
    drivers::cursor::init();

    #[cfg(target_arch = "x86_64")]
    drivers::ps2mouse::init();

    #[cfg(target_arch = "aarch64")]
    {
        // KMI devices are optional on QEMU virt - require explicit -device pl050
        // Keyboard input is available via PL011 UART console
        log::info!("aarch64: KMI devices not initialized (use -device pl050 to enable)");
    }

    scheduler::init();
    ipc::init();
    shm::init();
    compat::init();
    fs::init();
    fs::vfs_ops::init();
    userland::init();
    syscalls::init();
    net::init();
    pty::init();
    time::init();
    audio::init();

    log::info!("Spawning GUI and shell tasks.");

    // Attempt to spawn user-space WindowServer and DesktopShell.
    // When the feature "userspace_gui" is set (via build.rs detecting the ELF binaries),
    // we embed and spawn the user-space apps. Otherwise fall back to kernel tasks.
    #[cfg(feature = "userspace_gui")]
    {
        let ws_elf = include_bytes!("../../target/vibeos-x86_64/release/windowserver");
        let shell_elf = include_bytes!("../../target/vibeos-x86_64/release/desktop_shell");

        let ws_pid = spawn_userspace_app("windowserver", ws_elf);
        let shell_pid = spawn_userspace_app("desktop_shell", shell_elf);

        if ws_pid == 0 {
            log::warn!("WindowServer ELF load failed, falling back to kernel gui_task");
            scheduler::spawn(gui_task);
        } else {
            log::info!("User-space WindowServer spawned as pid={}", ws_pid);
        }

        if shell_pid == 0 {
            log::warn!("DesktopShell ELF load failed, falling back to kernel shell_task");
            scheduler::spawn(shell_task);
        } else {
            log::info!("User-space DesktopShell spawned as pid={}", shell_pid);
        }

        #[cfg(feature = "userspace_terminal")]
        {
            let terminal_elf = include_bytes!("../../target/vibeos-x86_64/release/terminal");
            let terminal_pid = spawn_userspace_app("terminal", terminal_elf);
            if terminal_pid == 0 {
                log::warn!("Terminal ELF load failed, terminal not available");
            } else {
                log::info!("User-space Terminal spawned as pid={}", terminal_pid);
            }
        }
    }

    #[cfg(not(feature = "userspace_gui"))]
    {
        log::info!("User-space GUI binaries not found, using kernel tasks");
        scheduler::spawn(gui_task);
        scheduler::spawn(shell_task);
    }

    log::info!("Tasks spawned, running scheduler.");
    scheduler::run_scheduler();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    arch_impl::halt_loop();
}
