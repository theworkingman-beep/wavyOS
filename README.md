# Aperture OS

Aperture OS is an experimental operating system written in Rust, designed from
the ground up for **native Windows application compatibility**, a modern
**hardware-accelerated GUI**, and **symmetric x86_64 + AArch64 support**.

> ⚠️ This is early bring-up code. It boots in QEMU (x86_64), prints to serial,
> draws to the framebuffer, receives keyboard input, and can load a synthetic
> PE executable into a ring-3 process, dispatch its NT system calls, and return
> to idle.

## Repository

The project was previously hosted as `wavyOS` and is now developed at
**`theworkingman-beep/ApertureOS`**.

## Goals

- **Better Windows app support than Wine/Proton or Windows itself**: implement
  the NT kernel ABI natively in Rust, with a clean Win32 subsystem and deep OS
  integration instead of wrapping another OS.
- **First-class cross-architecture support**: run x86, x86_64, and ARM64 PE
  binaries on either x86_64 or AArch64 hosts through native execution, a
  baseline JIT, or an interpreter.
- **Modern GUI**: a compositing window manager that can drive both software and
  future GPU-accelerated rendering.
- **Rust all the way down**: kernel, drivers, user-mode runtime, and Win32
  compatibility layer are written in safe Rust where possible.

## Build

Requires the nightly Rust toolchain, `rust-src`, and `llvm-tools-preview`:

```bash
rustup toolchain install nightly --component rust-src,llvm-tools-preview
rustup target add x86_64-unknown-none --toolchain nightly
rustup target add aarch64-unknown-none --toolchain nightly
```

Build the x86_64 kernel and bootable disk images:

```bash
./build.sh x86_64
```

Build the AArch64 kernel ELF (boot image generation not yet implemented):

```bash
ARCH=aarch64 ./build.sh
```

Run the host-testable PE parser unit tests:

```bash
cargo test -p pe-parser
```

Run in QEMU (requires `qemu-system-x86_64`):

```bash
./run-qemu.sh
```

## Architecture

```text
kernel/
  arch/        Hardware abstraction layer (x86_64, AArch64)
    x86_64/    IDT/PIC, context switch, SYSCALL/SYSRET
    aarch64/   Stubs for cross-architecture builds
  boot_info.rs Architecture-independent boot metadata
  gui/         Framebuffer compositor, bitmap font, text rendering
  mm/          Bump heap, bitmap frame allocator, x86_64 page tables
  vfs/         In-memory virtual filesystem backing NT file syscalls
  win32/       Windows compatibility subsystem
    abi/       x86/ARM translation layers and syscall helper
    loader.rs  PE/COFF loader + synthetic test executable
    nt.rs      NT syscall numbers, dispatch table, and handlers
    objects.rs Object manager / handle table
    process.rs Process model
    scheduler.rs Single-core cooperative scheduler
    thread.rs  Thread model
    registry.rs Registry shim
    win32k.rs  Win32 desktop/GUI bridge
tools/
  bootimage/   Host tool that wraps kernel ELF into BIOS/UEFI disk images
```

## Roadmap

1. [x] Bootable x86_64 skeleton with serial output and framebuffer
2. [x] HAL, early heap, GUI compositor, and Windows subsystem skeleton
3. [x] IDT/PIC interrupts, timer, and keyboard input
4. [x] Bitmap frame allocator and virtual memory page tables
5. [x] Cooperative scheduler and x86_64 context switch
6. [x] SYSCALL/SYSRET entry and NT syscall dispatch table
7. [x] PE loader that maps images into per-process address spaces
8. [ ] Preemptive multitasking and SMP bring-up
9. [ ] Cross-architecture binary translation (JIT + interpreter)
10. [ ] Full NT syscall coverage and Win32 API server
11. [ ] VFS persistence, registry, and driver model
12. [ ] Hardware-accelerated GUI compositor

## License

MIT OR Apache-2.0
