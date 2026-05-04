# VibeOS Development Continuation Prompt

Copy-paste this entire file to an AI assistant to pick up OS development.

---

## Context

I am developing **VibeOS** — a macOS-like GUI operating system written in Rust that boots via UEFI and supports both x86_64 and aarch64. The goal is a Mach-O/mac app compatibility layer with a user-space WindowServer, desktop shell, and sample apps.

**Repo:** `/home/b/vibe-coded-os` (also at `github.com/theworkingman-beep/vibe-coded-os`)

**Current version:** 0.1.0

## Architecture

- **UEFI Bootloader** (`bootloader/`): Reads `kernel` ELF from FAT filesystem, allocates at expected virtual address `0x10000000`, exits boot services, switches stack, jumps to `kernel_main(BootInfo*)`. Supports both x86_64 (`BOOTX64.EFI`) and aarch64 (`BOOTAA64.EFI`).
- **Kernel** (`kernel/`): `#![no_std]` monolithic kernel with cooperative scheduler, framebuffer renderer, PS/2 keyboard driver, IDT/PIC (x86_64), syscalls, IPC mailbox, shared memory, ELF loader, Mach-O compatibility layer.
- **User-space** (`apps/`): `libvibe` (graphics/IPC library), `windowserver` (compositor), `desktop_shell` (UI), `sample_app` (demo). Built with `-Zbuild-std=core,alloc` against custom JSON targets.
- **Shared types** (`common/`): `BootInfo`, `MemoryRegion`, `FramebufferInfo` structs passed from bootloader to kernel.

## What Works

- **x86_64 boots end-to-end** in QEMU with OVMF firmware
  - Bootloader loads kernel, exits boot services, jumps to `kernel_main`
  - Kernel inits UART logger (serial console), framebuffer (GOP), IDT+PIC, heap, scheduler
  - Desktop renders: menu bar ("VibeOS"), dock with colored icons, traffic-light window, system info
  - Timer IRQ0 fires (scheduler tick), keyboard IRQ1 registered (scancode handler ready)
  - IPC mailbox and shared memory allocator initialized
  - `scheduler::spawn()` supports GUI task + shell task
  - Syscall dispatch via `int 0x80` (codes 0-9 + 0x700 MachOExec)
  - ELF64 loader and Mach-O segment mapper implemented

- **aarch64 compiles** for all targets (kernel, bootloader, user-space apps)
  - Custom rustc target: `targets/vibeos-aarch64.json`
  - Uses `rust-lld` linker (no host LLVM path hardcoded)
  - Bootloader has proper `#[cfg(target_arch)]` guards for arch-specific transition code
  - Syscall convention: `svc #0` (x8=syscall)

- **Build system**: `./scripts/build.sh` creates 64MB FAT disk image + bootable ISO via `grub-mkrescue` + `mtools`

- **CI**: `.github/workflows/auto-release.yml` auto-bumps version by 0.0.1 per push, builds both arches, creates GitHub release

## Key Technical Details

- **Rust toolchain**: `nightly-aarch64-unknown-linux-gnu` at `$HOME/.rustup/toolchains/nightly-aarch64-unknown-linux-gnu/bin/`
- **Kernel base address**: `0x10000000` (256 MB) — set in `kernel/linker.ld`
- **Heap**: `linked_list_allocator`, skips kernel region, reserves 1 MB headroom
- **Framebuffer**: UEFI GOP, 1280x800 on test VM, direct pixel access via `drivers/fbcon.rs`
- **Scheduler**: Cooperative round-robin, context switching via saved register state
- **Syscalls**: x86_64 `int 0x80` (rax=syscall), aarch64 `svc #0` (x8=syscall)
- **Boot command (x86_64)**:
  ```
  qemu-system-x86_64 -machine q35 -m 512M \
    -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
    -drive if=pflash,format=raw,unit=1,file=OVMF_VARS_4M.fd \
    -drive format=raw,file=vibe-os-x86_64.iso
  ```
- **Boot command (aarch64)**:
  ```
  qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 512M \
    -drive if=pflash,format=raw,unit=0,file=/usr/share/AAVMF/AAVMF_CODE.fd,readonly=on \
    -drive if=pflash,format=raw,unit=1,file=AAVMF_VARS.fd \
    -drive format=raw,file=vibe-os-aarch64.iso
  ```

## Current State of Components

### Working (x86_64)
| Component | Status |
|-----------|--------|
| UEFI bootloader | Working |
| Kernel entry + init | Working |
| Framebuffer renderer (gradients, rounded rects, circles, 8x16 font) | Working |
| macOS-like desktop (menu bar, dock, window with traffic lights) | Working |
| IDT + PIC remap + IRQ handlers | Working |
| PS/2 keyboard driver (scancode set 1, ring buffer) | Compiled, IRQ registered |
| Heap allocator | Working (~231 MB) |
| Scheduler (cooperative round-robin) | Working |
| IPC mailbox | Initialized |
| Shared memory allocator | Initialized |
| Syscall dispatch | Implemented |
| ELF64 loader | Implemented |
| Mach-O compatibility layer | Implemented (segment mapping) |
| Build script + disk image + ISO | Working |

### Needs Work
| Component | Status |
|-----------|--------|
| aarch64 boot in QEMU | Compiles, not yet tested in QEMU |
| User-space process execution | ELF loader exists but not invoked yet |
| Mach-O execution | Compatibility layer exists but not invoked yet |
| Preemptive scheduling | Currently cooperative only |
| Keyboard input to shell | Driver exists, not wired to shell |
| Window management | In-kernel rendering, user-space WindowServer not yet integrated |
| VFS / filesystem | Not implemented |
| Memory management (paging, user-space isolation) | Not implemented |

## Directory Structure

```
vibe-coded-os/
├── Cargo.toml              # Workspace, all members
├── bootloader/
│   ├── Cargo.toml          # uefi + uefi-services deps
│   └── src/main.rs         # UEFI bootloader (x86_64 + aarch64)
├── common/
│   ├── Cargo.toml
│   └── src/lib.rs          # BootInfo, MemoryRegion, FramebufferInfo
├── kernel/
│   ├── Cargo.toml
│   ├── linker.ld           # ENTRY(kernel_main), base 0x10000000
│   └── src/
│       ├── main.rs         # kernel_main(), draw_desktop(), gui_task(), shell_task()
│       ├── arch/
│       │   ├── mod.rs
│       │   ├── x86_64.rs   # IDT, PIC, IRQ handlers, jump_to_user
│       │   └── aarch64.rs  # Stub (halt_loop, jump_to_user)
│       ├── mm/mod.rs       # Heap init (linked_list_allocator)
│       ├── scheduler/mod.rs # Task state, context switch, round-robin
│       ├── syscalls/mod.rs # int 0x80 / svc #0 dispatch (codes 0-9, 0x700)
│       ├── ipc.rs          # Mailbox-based message passing
│       ├── shm.rs          # Shared memory regions
│       ├── compat/
│       │   ├── mod.rs
│       │   ├── macho.rs    # Mach-O parsing + segment mapping
│       │   └── dyld.rs     # Dynamic loader stub
│       ├── drivers/
│       │   ├── mod.rs
│       │   ├── uart.rs     # Serial console
│       │   ├── uart_logger.rs # log crate integration
│       │   ├── fbcon.rs    # Framebuffer renderer
│       │   └── ps2kbd.rs   # PS/2 keyboard driver
│       └── userland/
│           ├── mod.rs
│           ├── loader.rs   # ELF64 parser
│           ├── shell.rs    # Shell implementation
│           ├── brew.rs     # Package manager stub
│           └── compositor.rs # Compositor stub
├── apps/
│   ├── libvibe/            # User-space graphics + IPC library
│   ├── windowserver/       # Compositor server
│   ├── desktop_shell/      # Desktop UI
│   └── sample_app/         # Demo app
├── targets/
│   ├── vibeos-x86_64.json  # Custom rustc target
│   └── vibeos-aarch64.json # Custom rustc target
├── hal/                    # Hardware abstraction layer
├── drivers/                # Standalone drivers crate
├── scripts/build.sh        # Build orchestration
└── .github/workflows/
    ├── rust.yml            # PR/merge build check
    └── auto-release.yml    # Auto version bump + release
```

## Build Commands

```bash
# Full build (both arches, disk image + ISO)
TARGET_ARCH=x86_64 ./scripts/build.sh
TARGET_ARCH=aarch64 ./scripts/build.sh

# QEMU test (set QEMU_RUN=1 in build.sh or run manually)
qemu-system-x86_64 -machine q35 -m 512M \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=OVMF_VARS_4M.fd \
  -drive format=raw,file=build/vibe-os-x86_64.img

# Kernel only
cargo build --target x86_64-unknown-none --release --package kernel

# Bootloader only
cargo build --target x86_64-unknown-uefi --release --package bootloader
cargo build --target aarch64-unknown-uefi --release --package bootloader

# User-space apps
cargo +nightly build -Zbuild-std=core,alloc -Zjson-target-spec \
  --target targets/vibeos-x86_64.json --release --package windowserver
```

## Next Steps (Prioritized)

1. **Wire up keyboard input** — PS/2 driver has `handle_scancode()` and ring buffer; connect to shell task for interactive input
2. **Execute user-space ELF** — Hook up `userland::loader.rs` to spawn actual user-space processes from the disk image
3. **Implement VFS** — Basic FAT filesystem reader in kernel so apps can be loaded by name
4. **User-space memory isolation** — Set up page tables with user/kernel split (ring 3 / EL0)
5. **Integrate WindowServer** — Move compositor from in-kernel `draw_desktop()` to user-space `windowserver` app with IPC
6. **Mach-O execution** — Wire up `compat/macho.rs` to load and execute Mach-O binaries
7. **Preemptive scheduling** — Use timer IRQ0 to forcibly yield after a time slice
8. **aarch64 boot testing** — Test in QEMU with AAVMF firmware, implement PL011 UART for serial console

## Credentials & Config

- **sudo password** and **GitHub credentials**: stored in `/home/b/credentials.txt`
- **Git author**: `theworkingman-beep <280896895+theworkingman-beep@users.noreply.github.com>`
- **OVMF firmware**: `/usr/share/OVMF/OVMF_CODE_4M.fd`, `/usr/share/OVMF/OVMF_VARS_4M.fd`
- **AAVMF firmware**: `/usr/share/AAVMF/AAVMF_CODE.fd`, `/usr/share/AAVMF/AAVMF_VARS.fd`
