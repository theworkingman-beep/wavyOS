//! NT system call dispatch table.
//!
//! This module defines the syscall numbers and dispatch stubs for the most
//! important NT kernel calls. Each entry either maps to a native Aperture OS
//! implementation or returns STATUS_NOT_IMPLEMENTED during early bring-up.

use core::sync::atomic::{AtomicBool, Ordering};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    INITIALIZED.store(true, Ordering::Relaxed);
}

/// NT status codes used by the dispatch layer.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NtStatus {
    Success = 0x00000000,
    Pending = 0x00000103,
    NotImplemented = 0xC0000002,
    InvalidParameter = 0xC000000D,
    AccessDenied = 0xC0000022,
    BufferTooSmall = 0xC0000023,
    ObjectNameNotFound = 0xC0000034,
}

/// NT system call numbers (subset). Windows keeps these stable per architecture.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyscallNumber {
    NtCreateFile = 0x0055,
    NtClose = 0x000F,
    NtReadFile = 0x006F,
    NtWriteFile = 0x0008,
    NtAllocateVirtualMemory = 0x0018,
    NtFreeVirtualMemory = 0x001E,
    NtCreateProcess = 0x004F,
    NtCreateThread = 0x004E,
    NtQuerySystemInformation = 0x0036,
    NtQueryInformationProcess = 0x0019,
    NtSetInformationProcess = 0x001C,
    NtDelayExecution = 0x0034,
    NtCreateKey = 0x001D,
    NtQueryValueKey = 0x0049,
    NtSetValueKey = 0x005D,
    NtWaitForMultipleObjects = 0x0001,
}

/// Syscall dispatch stub. The real entry point will marshal arguments from
/// the user-mode trap frame and call the appropriate handler.
pub fn dispatch(_number: SyscallNumber, _args: [usize; 16]) -> NtStatus {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return NtStatus::AccessDenied;
    }
    NtStatus::NotImplemented
}
