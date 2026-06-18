# Aperture OS Architecture

This document describes the high-level design of Aperture OS. It is written to
help contributors understand how the pieces fit together as the project grows.

## 1. Kernel

The kernel is a monolithic, single-address-space design for early bring-up.
Over time it will grow proper process isolation, virtual memory, and a
microkernel-like driver boundary.

### 1.1 Hardware Abstraction Layer (`kernel/src/arch/`)

Each supported CPU architecture implements a small uniform interface:

- `init()` — architecture-specific hardware setup
- `debug_putchar(u8)` — serial/debug output
- `hlt() -> !` — halt forever

x86_64 uses `x86_64` crate and port I/O. AArch64 uses inline assembly and will
use PL011 or semihosting for early output.

### 1.2 Memory Management (`kernel/src/mm/`)

Current state: a simple bump allocator for early heap allocations.

Planned: bitmap frame allocator + slab/page allocator + per-architecture page
tables.

### 1.3 GUI (`kernel/src/gui/`)

The GUI is a software-rendered compositor. It owns the bootloader framebuffer
and a list of windows. Each window has a premultiplied RGBA backbuffer. The
compositor blends windows back-to-front.

Future: GPU command buffers, hardware overlays, and a Wayland-like protocol
for user-mode clients.

### 1.4 Windows Compatibility (`kernel/src/win32/`)

Aperture OS does not wrap Wine. It implements the NT kernel ABI directly:

- **PE loader** parses x86/x64/ARM64 PE images and prepares them for execution.
- **NT syscall dispatch** routes user-mode traps to native implementations.
- **Object manager** provides handles for processes, threads, files, keys, and
desktops.
- **Win32k** bridges NT objects to the Aperture OS GUI compositor.
- **Registry shim** provides the HKCU/HKLM hives in memory.
- **Translation layer** runs mismatched-architecture binaries through a JIT or
interpreter.

## 2. Boot Tool

`tools/bootimage` is a host-side Rust program that uses the `bootloader` crate
to wrap the kernel ELF into BIOS and UEFI disk images. It is excluded from the
kernel workspace so host `std` crates do not conflict with the kernel's
`build-std` setup.

## 3. Cross-Architecture Strategy

Aperture OS will ship two kernel builds (x86_64 and AArch64). User-mode binaries
can be any architecture:

- Native arch: direct execution.
- Different arch: a per-process translator (interpreter for cold code,
baseline JIT for hot code) with host-native thunks for syscalls and GUI calls.
