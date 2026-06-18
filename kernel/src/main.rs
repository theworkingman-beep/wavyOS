#![no_std]
#![no_main]

#[cfg(feature = "arch_x86_64")]
mod x86_64_entry {
    use bootloader_api::config::Mapping;
    use bootloader_api::{entry_point, BootInfo, BootloaderConfig};

    pub static BOOTLOADER_CONFIG: BootloaderConfig = {
        let mut config = BootloaderConfig::new_default();
        config.mappings.physical_memory = Some(Mapping::Dynamic);
        config.kernel_stack_size = 64 * 1024; // 64 KiB
        config
    };

    fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
        kernel::init();
        kernel::logln!("Aperture OS x86_64 kernel booting...");

        let mut usable = None;
        for region in boot_info.memory_regions.iter() {
            let region: kernel::boot_info::MemoryRegion = (*region).into();
            if region.kind == kernel::boot_info::MemoryRegionKind::Usable
                && region.end - region.start >= 0x10_0000
            {
                usable = Some(region);
                break;
            }
        }
        let mut regions = [kernel::boot_info::MemoryRegion::default(); 64];
        let mut region_count = 0usize;
        for region in boot_info.memory_regions.iter() {
            if region_count < regions.len() {
                regions[region_count] = (*region).into();
                region_count += 1;
            }
        }
        unsafe {
            kernel::mm::init_physical_allocator(&regions[..region_count]);
            kernel::mm::page_table::capture_kernel_page_table();
        }
        kernel::logln!("Physical frame allocator initialized.");

        if let Some(region) = usable {
            kernel::mm::init_heap(region.start, region.end);
            kernel::logln!(
                "Early heap: {:#x} - {:#x} ({} MiB)",
                region.start,
                region.end,
                (region.end - region.start) / 1024 / 1024
            );
        }

        kernel::win32::self_test();

        let mut win = None;
        if let Some(fb) = boot_info.framebuffer.as_mut() {
            let info: kernel::boot_info::FrameBufferInfo = fb.info().into();
            let len = fb.buffer_mut().len();
            let buffer = unsafe {
                core::slice::from_raw_parts_mut(fb.buffer_mut().as_mut_ptr(), len)
            };
            kernel::gui::init_compositor(buffer, info);
            win = kernel::gui::create_window("Aperture", 100, 100, 320, 240);
            kernel::gui::draw_text(
                win,
                "Aperture OS",
                12,
                12,
                kernel::gui::Color::new(0xFF, 0xFF, 0xFF),
            );
            kernel::gui::draw_text(
                win,
                "Hello, world!",
                12,
                28,
                kernel::gui::Color::new(0x00, 0xFF, 0x00),
            );
            kernel::gui::render();
            kernel::logln!(
                "Framebuffer: {}x{} stride={} bpp={}",
                info.width,
                info.height,
                info.stride,
                info.bytes_per_pixel
            );
        } else {
            kernel::logln!("No framebuffer available.");
        }

        kernel::logln!("Kernel idle; reading keyboard input.");
        let mut input = [0u8; 80];
        let mut input_len = 0usize;
        loop {
            while let Some(ch) = kernel::arch::interrupts::read_char() {
                match ch {
                    '\n' => input_len = 0,
                    '\u{8}' => {
                        if input_len > 0 {
                            input_len -= 1;
                        }
                    }
                    _ => {
                        if input_len < input.len() && ch.is_ascii() {
                            input[input_len] = ch.to_ascii_uppercase() as u8;
                            input_len += 1;
                        }
                    }
                }
            }

            kernel::gui::clear_window(win, kernel::gui::Color::DARK_GRAY);
            kernel::gui::draw_text(win, "Aperture OS", 12, 12, kernel::gui::Color::new(0xFF, 0xFF, 0xFF));
            kernel::gui::draw_text(win, "Hello, world!", 12, 28, kernel::gui::Color::new(0x00, 0xFF, 0x00));
            let line = core::str::from_utf8(&input[..input_len]).unwrap_or("");
            kernel::gui::draw_text(win, "Input: ", 12, 50, kernel::gui::Color::new(0xFF, 0xFF, 0xFF));
            kernel::gui::draw_text(win, line, 60, 50, kernel::gui::Color::new(0x00, 0xFF, 0x00));
            kernel::gui::render();

            kernel::arch::halt_once();
        }
    }

    entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);
}

#[cfg(feature = "arch_aarch64")]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    kernel::init();
    kernel::logln!("Aperture OS AArch64 kernel booting...");

    // Early bring-up uses a hardcoded usable memory region. A real AArch64
    // boot path will parse the device tree / UEFI memory map instead.
    let region = kernel::boot_info::MemoryRegion {
        start: 0x4000_0000,
        end: 0x4100_0000,
        kind: kernel::boot_info::MemoryRegionKind::Usable,
    };
    unsafe {
        kernel::mm::init_physical_allocator(core::slice::from_ref(&region));
    }
    kernel::logln!("Physical frame allocator initialized (hardcoded region).");
    kernel::mm::init_heap(region.start, region.end);
    kernel::logln!("Early heap: {:#x} - {:#x} ({} MiB)", region.start, region.end, 16);

    kernel::win32::self_test();

    kernel::logln!("No framebuffer on this boot path yet.");
    kernel::logln!("Kernel idle.");
    kernel::hlt();
}
