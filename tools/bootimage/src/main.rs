use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: aperture-bootimage <kernel-elf> <uefi-image> <bios-image>");
        std::process::exit(1);
    }

    let kernel_elf = PathBuf::from(&args[1]);
    let uefi_path = PathBuf::from(&args[2]);
    let bios_path = PathBuf::from(&args[3]);

    if !kernel_elf.exists() {
        eprintln!("Kernel ELF not found: {}", kernel_elf.display());
        std::process::exit(1);
    }

    let builder = bootloader::DiskImageBuilder::new(kernel_elf);
    builder
        .create_uefi_image(&uefi_path)
        .expect("Failed to create UEFI disk image");
    builder
        .create_bios_image(&bios_path)
        .expect("Failed to create BIOS disk image");

    println!("UEFI image: {}", uefi_path.display());
    println!("BIOS image: {}", bios_path.display());
}
