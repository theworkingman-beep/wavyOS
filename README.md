# Aperture OS

Aperture OS is an experimental operating system written in Rust, designed from
the ground up for **native Windows application compatibility**, a modern
**hardware-accelerated GUI**, and **symmetric x86_64 + AArch64 support**.

> ⚠️ This is early bring-up code. It boots in QEMU, prints to serial, draws to
the framebuffer, and has skeletons for memory management, interrupt handling,
and the Windows compatibility subsystem.

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
```

Build the x86_64 kernel and bootable disk images:

```bash
./build.sh
```

Run in QEMU (requires `qemu-system-x86_64`):

```bash
./run-qemu.sh
```

## Architecture

```text
kernel/
  arch/        Hardware abstraction layer (x86_64, AArch64)
  gui/         Framebuffer compositor and windowing
  mm/          Memory management
  win32/       Windows compatibility subsystem
    abi/       x86/ARM translation layers
    loader.rs  PE/COFF loader
    nt.rs      NT syscall dispatch
    objects.rs Object manager / handle table
    process.rs Process model
    thread.rs  Thread model
    registry.rs Registry shim
    win32k.rs  Win32 desktop/GUI bridge
tools/
  bootimage/   Host tool that wraps kernel ELF into BIOS/UEFI disk images
```

## Roadmap

1. [x] Bootable x86_64 skeleton with serial output and framebuffer
2. [x] HAL, early heap, GUI compositor, and Windows subsystem skeleton
3. [ ] Interrupts, timer, and preemptive multitasking
4. [ ] Page frame allocator and virtual memory
5. [ ] Cross-architecture binary translation (JIT + interpreter)
6. [ ] NT syscall implementation and PE section loading
7. [ ] Win32 API server and message queue
8. [ ] VFS, registry persistence, and driver model
9. [ ] Hardware-accelerated GUI compositor

## License

MIT OR Apache-2.0
