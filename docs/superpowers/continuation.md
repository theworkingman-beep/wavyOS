# VibeOS Continuation

## Instructions for the AI

You are the autonomous developer for VibeOS. Your goal is to turn this into a fully working macOS-compatible daily driver operating system on both ARM64 and AMD64.

### Workflow

1. **Work through the TODO list below in order.** Remove a TODO item once it is fully implemented, tested, and a release has been created.
2. **After each TODO is completed:** Push to main. The CI (`.github/workflows/auto-release.yml`) will automatically build both architectures and create a GitHub release.
3. **Monitor the CI build:** Check that the GitHub Actions run succeeds. If it fails, fix the issue and push again.
4. **Test in VM:** Once the release is built, download the ISO and test the new features in QEMU:
   - x86_64: `qemu-system-x86_64 -machine q35 -m 512M -drive if=pflash,format=raw,unit=0,file=/usr/share/OVMF/OVMF_CODE_4M.fd,readonly=on -drive if=pflash,format=raw,unit=1,file=OVMF_VARS_4M.fd -drive format=raw,file=vibe-os-x86_64.iso`
   - aarch64: `qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 512M -drive if=pflash,format=raw,unit=0,file=/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd,readonly=on -drive if=pflash,format=raw,unit=1,file=/usr/share/AAVMF/AAVMF_VARS.fd -drive format=raw,file=vibe-os-aarch64.iso`
5. **If everything works:** Move to the next TODO item.
6. **When the TODO list is empty:** Add new TODO items that advance the goal of making VibeOS a fully macOS-compatible daily driver OS. Think about what features are needed for real-world usage: file systems, networking, proper process management, ELF/Mach-O loading, syscall interface, user-space drivers, GUI improvements, etc.
7. **Make all decisions independently.** You do not need to ask for permission. Work indefinitely until the OS is a true macOS replacement with full app compatibility.

### Cutting Corners Is Allowed

You are explicitly allowed and encouraged to cut corners where it makes sense. Use existing open source libraries instead of writing everything from scratch. For example:
- Use **OpenSSL** or **ring** for cryptography instead of writing your own crypto
- Use existing **lwIP**, **smoltcp**, or other TCP/IP stack implementations instead of writing networking from scratch
- Use **FatFS** or existing FAT32 implementations instead of writing a filesystem driver from scratch
- Use existing **UEFI**, **ACPI**, **USB** libraries and reference implementations
- Port existing open source drivers, libraries, and frameworks rather than reinventing them
- The goal is a working macOS-compatible OS, not a pure from-scratch exercise. Borrow, adapt, and integrate aggressively.

### Tools & Environment

- **Superpowers plugin:** Installed via `opencode.json`. Use `/using-superpowers` skill to manage workflows.
- **If superpowers is not installed:** Follow installation at https://github.com/obra/superpowers?tab=readme-ov-file#installation
- **CLI permissions:** If opencode (or any other CLI) does not have full permissions, configure it to have all permissions set to `"allow"` and prompt the user to restart the CLI for changes to take effect.
- **Rust toolchain:** `nightly-aarch64-unknown-linux-gnu` at `$HOME/.rustup/toolchains/nightly-aarch64-unknown-linux-gnu/bin/`
- **OVMF:** `/usr/share/OVMF/OVMF_CODE_4M.fd` + `/usr/share/OVMF/OVMF_VARS_4M.fd`
- **AAVMF:** `/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd` + `/usr/share/AAVMF/AAVMF_VARS.fd`
- **Git credentials:** Author `theworkingman-beep <280896895+theworkingman-beep@users.noreply.github.com>`
- **GitHub:** `theworkingman-beep/vibe-coded-os`
- **Sudo password and GitHub credentials:** stored in `/home/b/credentials.txt`

### Key Project Info

- **Architecture:** UEFI-booted kernel (`no_std` Rust), UEFI bootloader loads kernel via memory map, cooperative scheduler with global_asm context switch
- **Current state:** Boots on both x86_64 and aarch64 with GUI desktop (menu bar, dock, traffic-light windows), cooperative multitasking, UART serial logging, framebuffer rendering
- **Build:** `./scripts/build.sh` produces versioned `.iso` and `.img` files
- **CI:** Auto-release on push to main — bumps version, builds both arches in parallel, creates GitHub release with artifacts

---

## TODO List

- [x] **Implement PS/2 mouse driver (x86_64)** — IRQ 12 handler, 3-byte packet decoding, MouseState struct, push MouseMove/MouseDown/MouseUp events to input subsystem. Commit → CI release → test mouse in QEMU x86_64.
- [x] **Implement cursor renderer** — 16x16 arrow bitmap, save/restore pixels under cursor, draw/undraw/move_cursor API, position clamping. Commit → CI release → test cursor visible and moves with mouse.
- [x] **Implement hit-test system (wm.rs)** — TrafficLight (close/min/max), DockIcon, TitleBar, WindowBody hit targets. Pure logic, testable on host. Commit → CI release.
- [x] **Refactor gui_task into event-driven compositor** — Poll input events, handle MouseMove (cursor + dragging), MouseDown (hit-test + dispatch), MouseUp (end drag). Commit → CI release → test click dock, drag windows, close window.
- [x] **Add window dragging** — Track drag offset on title bar MouseDown, update window position on MouseMove during drag, end drag on MouseUp. Redraw window at new position.
- [x] **Add dock hover highlighting** — Detect cursor over dock icons, highlight with white overlay, clear highlight when cursor leaves.
- [x] **Add cursor black outline** — Draw 1px black border around white cursor pixels for visibility on any background.
- [x] **Add traffic light minimize/maximize** — Minimize hides window (dock icon click restores), maximize fills screen, click again to restore original size.
- [x] **Wire KeyPress to shell** — GUI task re-pushes KeyPress events so shell task receives them from shared input ring buffer.
- [x] **Implement aarch64 GIC initialization + IRQ handling** — GICv2 init (distributor + CPU interface), exception vector table (VBAR_EL1), IRQ EL1 handler assembly, Rust IRQ dispatcher. Commit → CI release → test boots on aarch64 without crash.
- [x] **Implement PL050 KMI mouse driver (aarch64)** — MMIO driver with PS/2 protocol, IRQ dispatch through GIC. Implemented but disabled by default (KMI requires `-device pl050` on QEMU virt). Keyboard available via PL011 UART IRQ.
- [x] **Wire keyboard input to shell via input subsystem** — Update ps2kbd.rs to push KeyPress events, update shell to read from input::poll() instead of UART. Commit → CI release → test typing in shell on both architectures.
- [x] **Implement a proper file system (FAT32 + VFS abstraction)** — VFS layer with `FileSystem` trait, `open`/`read`/`read_dir` API. FAT32 read-only driver with VFAT long filename support, cluster chain following, directory traversal. VirtIO block device driver skeleton. Block access not yet wired to actual disk (needs PCI BAR mapping + virtqueue setup).
- [ ] **Implement virtual memory / paging for user-space** — Page tables, page fault handler, copy-on-write, user/kernel memory separation. Commit → CI release.
- [ ] **Implement syscall interface** — Syscall numbers, user→kernel transition, argument passing, return values. Commit → CI release.
- [ ] **Implement user-space process management** — Fork, exec, exit, wait, PID allocation, process table. Commit → CI release.
- [ ] **Implement ELF loader for user-space binaries** — Parse ELF64, map segments, set up stack, jump to entry point. Commit → CI release → test running user-space ELF binary.
- [ ] **Implement Mach-O compatibility layer** — Parse Mach-O 64-bit, map segments, handle relocations, dynamic linking stubs. Commit → CI release → test running simple Mach-O binary.
- [ ] **Implement networking stack (TCP/IP)** — Ethernet driver (virtio-net for QEMU), ARP, IP, UDP, TCP, DNS. Commit → CI release → test network connectivity.
- [ ] **Implement user-space WindowServer** — Separate compositor process, window creation/destruction, input routing to focused window, double-buffered rendering. Commit → CI release.
- [ ] **Implement user-space terminal app** — PTY, shell as child process, text rendering with scrollback, keyboard input routing. Commit → CI release.
- [ ] **Implement libc compatibility layer** — Basic POSIX syscalls (open, read, write, malloc, exit, etc.) so existing C programs can be recompiled. Commit → CI release.
- [ ] **Implement dynamic linker (dyld compatible)** — Load dylibs, resolve symbols, run init functions, lazy binding. Commit → CI release.
- [ ] **Implement GUI framework (AppKit-like)** — Views, windows, events, drawing, buttons, text fields, menus. Commit → CI release.
- [ ] **Implement audio subsystem** — Audio driver (virtio-sound or HDA), mixer, PCM playback, user-space API. Commit → CI release.
- [ ] **Implement USB support** — xHCI host controller driver, HID class support for USB mouse/keyboard. Commit → CI release → test USB devices in QEMU.
- [ ] **Implement power management (ACPI)** — ACPI table parsing, power button handling, sleep/wake, battery status for laptops. Commit → CI release.
- [ ] **Implement proper bootloader with kernel selection** — Boot menu, kernel selection, cmdline passing, initrd support. Commit → CI release.
- [ ] **Add package manager** — Install/update/remove apps and system components, dependency resolution, package repository. Commit → CI release.
- [ ] **Implement full Mach-O + dyld compatibility** — Run unmodified macOS apps compiled for ARM64/AMD64, full Framework support, Cocoa/AppKit compatibility. Commit → CI release → test running real macOS applications.
