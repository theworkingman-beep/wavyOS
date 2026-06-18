//! NT system call dispatch and implementations.
//!
//! This module defines the syscall numbers, dispatch stub, and concrete
//! implementations for the most important NT kernel calls. The implementations
//! route to the Aperture OS VFS and memory allocator.

use super::{loader, objects};
use crate::vfs::{self, FileHandle, NodeKind};
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
    ObjectNameCollision = 0xC0000035,
    EndOfFile = 0xC0000011,
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

/// Open or create a file and return a kernel file handle.
pub fn create_file(
    path: &str,
    create: bool,
    _directory: bool,
    write_access: bool,
) -> Result<FileHandle, NtStatus> {
    let node = vfs::lookup(path);
    let node = match node {
        Some(n) => {
            if create {
                return Err(NtStatus::ObjectNameCollision);
            }
            n
        }
        None => {
            if !create {
                return Err(NtStatus::ObjectNameNotFound);
            }
            let parent_path = parent(path);
            let parent_node = vfs::lookup(parent_path).ok_or(NtStatus::ObjectNameNotFound)?;
            let name = file_name(path);
            vfs::create(parent_node, name, NodeKind::File).ok_or(NtStatus::InvalidParameter)?
        }
    };
    vfs::open(node, write_access).ok_or(NtStatus::InvalidParameter)
}

/// Read from an open file.
pub fn read_file(handle: FileHandle, buffer: &mut [u8]) -> Result<usize, NtStatus> {
    vfs::read(handle, buffer).ok_or(NtStatus::InvalidParameter)
}

/// Write to an open file.
pub fn write_file(handle: FileHandle, buffer: &[u8]) -> Result<usize, NtStatus> {
    vfs::write(handle, buffer).ok_or(NtStatus::InvalidParameter)
}

/// Close an open handle.
pub fn close(handle: FileHandle) -> NtStatus {
    if vfs::close(handle) {
        NtStatus::Success
    } else {
        NtStatus::InvalidParameter
    }
}

/// Allocate virtual memory for a process.
pub fn allocate_virtual_memory(size: usize) -> Result<u64, NtStatus> {
    let frames_needed = (size + 4095) / 4096;
    let mut base = 0u64;
    for _ in 0..frames_needed {
        let frame = crate::mm::frame_allocator::allocate().ok_or(NtStatus::InvalidParameter)?;
        if base == 0 {
            base = frame;
        }
    }
    Ok(base)
}

/// Free virtual memory allocated by `allocate_virtual_memory`.
pub fn free_virtual_memory(base: u64, size: usize) -> NtStatus {
    let frames = (size + 4095) / 4096;
    for i in 0..frames {
        crate::mm::frame_allocator::free(base + i as u64 * 4096);
    }
    NtStatus::Success
}

/// Load a PE executable from the VFS into a new process.
///
/// Returns the object handle for the created process and whether the guest
/// architecture requires binary translation on the host.
pub fn create_user_process(path: &str) -> Option<(objects::Handle, bool)> {
    let file = create_file(path, false, false, false).ok()?;
    let size = vfs::file_size(file)?;
    let buf = crate::mm::alloc_early(size, 1)?;

    let slice = unsafe { core::slice::from_raw_parts_mut(buf, size) };
    let read = vfs::read(file, slice)?;
    if read != size {
        return None;
    }

    let _ = vfs::close(file);
    loader::load_pe(slice, 1)
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(idx) => &path[..idx],
        None => "/",
    }
}

fn file_name(path: &str) -> &str {
    path.rfind('/').map(|idx| &path[idx + 1..]).unwrap_or(path)
}
