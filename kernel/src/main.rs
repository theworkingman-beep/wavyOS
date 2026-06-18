#![no_std]
#![no_main]

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

    kernel::logln!("Aperture OS kernel booting...");
    kernel::logln!("Bootloader API version: {:?}", boot_info.api_version);

    // Initialize memory manager with the first usable region.
    let mut usable: Option<bootloader_api::info::MemoryRegion> = None;
    for region in boot_info.memory_regions.iter() {
        if region.kind == bootloader_api::info::MemoryRegionKind::Usable && region.end - region.start >= 0x10_0000 {
            usable = Some(*region);
            break;
        }
    }
    if let Some(region) = usable {
        kernel::mm::init_heap(region.start, region.end);
        kernel::logln!(
            "Early heap: {:#x} - {:#x} ({} MiB)",
            region.start,
            region.end,
            (region.end - region.start) / 1024 / 1024
        );
    }

    // Initialize GUI compositor from the bootloader framebuffer.
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let info = fb.info();
        let buffer = fb.buffer_mut();
        let len = buffer.len();
        let static_buffer = unsafe { core::slice::from_raw_parts_mut(buffer.as_mut_ptr(), len) };
        kernel::gui::init_compositor(static_buffer, info);

        // Create a small demo window.
        let _win = kernel::gui::create_window("Aperture", 100, 100, 320, 240);

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

    kernel::logln!("Kernel idle.");
    loop {
        x86_64::instructions::hlt();
    }
}

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

