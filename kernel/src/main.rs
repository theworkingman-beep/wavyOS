//! Vibe Coded OS — Kernel Entry Point
#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;

pub use common::{BootInfo, FramebufferInfo, MemoryRegion, MemoryRegionKind};

mod arch;
mod mm;
mod scheduler;
mod syscalls;
mod ipc;
mod shm;
mod compat;
mod drivers;
mod userland;

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

/// GUI init task — draws the macOS-like desktop
extern "C" fn gui_task() -> ! {
    log::info!("gui_task: starting desktop compositor");
    draw_desktop();
    log::info!("gui_task: desktop rendered");

    // Main compositor loop
    loop {
        // Check for IPC messages (window updates, etc.)
        scheduler::yield_cpu();
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

    arch_impl::init(unsafe { &mut *boot_info });
    mm::init(mem_map);
    scheduler::init();
    ipc::init();
    shm::init();
    compat::init();
    userland::init();
    syscalls::init();

    log::info!("Spawning GUI and shell tasks.");
    scheduler::spawn(gui_task);
    scheduler::spawn(shell_task);

    scheduler::run_scheduler();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("KERNEL PANIC: {}", info);
    arch_impl::halt_loop();
}
