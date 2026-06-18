# Windows Compatibility Strategy

Aperture OS aims for first-class Windows binary compatibility by implementing
the NT kernel ABI natively. This is a long-term effort comparable in scope to
Wine or ReactOS, but built with a different architecture:

- **Kernel-mode NT personality** rather than a userspace compatibility layer.
- **Native Rust implementations** of NT objects, syscalls, and the object
  manager.
- **Deep GUI integration** via `win32k` bridging to the Aperture OS compositor.
- **Architecture flexibility** so x86/x64/ARM64 Windows apps run on any Aperture
  OS host CPU.

## Phases

### Phase 1: Loader and basic NT objects
- Parse PE headers for x86/x64/ARM64.
- Create process/thread objects and handle table.
- Map static sections into a process address space.

### Phase 2: Core syscalls
- Memory: `NtAllocateVirtualMemory`, `NtFreeVirtualMemory`, `NtProtectVirtualMemory`.
- Files: `NtCreateFile`, `NtReadFile`, `NtWriteFile`, `NtClose`.
- Synchronization: `NtWaitForMultipleObjects`, `NtDelayExecution`.
- Registry: `NtCreateKey`, `NtQueryValueKey`, `NtSetValueKey`.

### Phase 3: Win32 subsystem
- User-mode DLL loading and import resolution.
- Window stations, desktops, HWND/message queue model.
- GDI/USER integration with the compositor.

### Phase 4: Advanced compatibility
- NT object namespace, security descriptors, ACLs.
- COM/RPC runtime.
- DirectX/Vulkan graphics runtime mapped to native GPU APIs.
- .NET CLR support (or native AOT fallback).

## Why this can surpass Wine/Proton

Wine translates Win32 calls into POSIX/Linux calls. Aperture OS implements the
NT semantics natively, so:

- No impedance mismatch between NT and POSIX semantics.
- Kernel objects, handles, and waits behave exactly as Windows apps expect.
- The GUI stack is designed around composited windows from day one.
- x86-on-ARM and ARM-on-x86 translation is part of the kernel, not an extra
  layer like box64 or FEX.

## Why this can surpass Windows

Windows carries decades of backward-compatibility baggage. Aperture OS can:

- Implement only the NT interfaces actually needed by modern apps.
- Use a clean, safe Rust codebase without legacy driver constraints.
- Expose modern APIs (Vulkan, Wayland-style compositor protocols) directly to
  apps while still supporting Win32.
