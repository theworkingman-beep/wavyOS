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

impl SyscallNumber {
    /// Convert a raw syscall number to the enum, if known.
    pub fn from_raw(value: usize) -> Option<Self> {
        use SyscallNumber::*;
        Some(match value {
            0x0055 => NtCreateFile,
            0x000F => NtClose,
            0x006F => NtReadFile,
            0x0008 => NtWriteFile,
            0x0018 => NtAllocateVirtualMemory,
            0x001E => NtFreeVirtualMemory,
            0x004F => NtCreateProcess,
            0x004E => NtCreateThread,
            0x0036 => NtQuerySystemInformation,
            0x0019 => NtQueryInformationProcess,
            0x001C => NtSetInformationProcess,
            0x0034 => NtDelayExecution,
            0x001D => NtCreateKey,
            0x0049 => NtQueryValueKey,
            0x005D => NtSetValueKey,
            0x0001 => NtWaitForMultipleObjects,
            _ => return None,
        })
    }
}

impl From<usize> for SyscallNumber {
    fn from(value: usize) -> Self {
        Self::from_raw(value).unwrap_or(Self::NtWaitForMultipleObjects)
    }
}

/// Syscall dispatch table entry.
#[derive(Clone, Copy)]
struct SyscallHandler {
    func: fn([usize; 16]) -> NtStatus,
}

macro_rules! handler_table {
    ($(($num:expr, $fn:ident)),* $(,)?) => {
        {
            let mut table: [Option<SyscallHandler>; 256] = [None; 256];
            $(
                table[$num as usize] = Some(SyscallHandler { func: $fn });
            )*
            table
        }
    };
}

static mut SYSCALL_TABLE: [Option<SyscallHandler>; 256] = [None; 256];

/// Initialize the syscall dispatch table.
pub fn init_syscall_table() {
    unsafe {
        SYSCALL_TABLE = handler_table!(
            (SyscallNumber::NtClose, handle_close),
            (SyscallNumber::NtCreateFile, handle_create_file),
            (SyscallNumber::NtReadFile, handle_read_file),
            (SyscallNumber::NtWriteFile, handle_write_file),
            (SyscallNumber::NtAllocateVirtualMemory, handle_allocate_virtual_memory),
            (SyscallNumber::NtFreeVirtualMemory, handle_free_virtual_memory),
        );
    }
}

/// Syscall dispatch stub. The real entry point marshals arguments from the
/// user-mode trap frame and calls the appropriate handler.
pub fn dispatch(number: SyscallNumber, args: [usize; 16]) -> NtStatus {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return NtStatus::AccessDenied;
    }
    unsafe {
        if let Some(handler) = SYSCALL_TABLE[number as usize] {
            (handler.func)(args)
        } else {
            NtStatus::NotImplemented
        }
    }
}

fn handle_close(args: [usize; 16]) -> NtStatus {
    let handle = FileHandle(args[0]);
    close(handle)
}

fn handle_create_file(args: [usize; 16]) -> NtStatus {
    // Simplified ABI for bring-up tests: args[2] is a pointer to a
    // null-terminated path, args[7] is create disposition (nonzero = create),
    // args[1] is access mask (GENERIC_WRITE bit enables writing).
    let path_ptr = args[2] as u64;
    let out_handle_ptr = args[0] as u64;
    if path_ptr == 0 || out_handle_ptr == 0 {
        return NtStatus::InvalidParameter;
    }

    let path_phys = match unsafe { user_ptr_to_phys(path_ptr) } {
        Some(p) => p,
        None => return NtStatus::InvalidParameter,
    };
    let out_phys = match unsafe { user_ptr_to_phys(out_handle_ptr) } {
        Some(p) => p,
        None => return NtStatus::InvalidParameter,
    };

    let path = unsafe { guest_cstr(path_phys, 128) };
    let create = args[7] != 0;
    let write_access = (args[1] & 0x4000_0000) != 0 || create;

    match create_file(path, create, false, write_access) {
        Ok(handle) => {
            unsafe { core::ptr::write_volatile(out_phys as *mut u64, handle.0 as u64) };
            NtStatus::Success
        }
        Err(status) => status,
    }
}

fn handle_read_file(args: [usize; 16]) -> NtStatus {
    let handle = FileHandle(args[0]);
    let buf_ptr = args[2] as u64;
    let len = args[4];
    if buf_ptr == 0 || len == 0 {
        return NtStatus::Success;
    }
    let buf_phys = match unsafe { user_ptr_to_phys(buf_ptr) } {
        Some(p) => p,
        None => return NtStatus::InvalidParameter,
    };
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_phys as *mut u8, len) };
    match read_file(handle, buf) {
        Ok(_read) => NtStatus::Success,
        Err(status) => status,
    }
}

fn handle_write_file(args: [usize; 16]) -> NtStatus {
    let handle = FileHandle(args[0]);
    let buf_ptr = args[2] as u64;
    let len = args[4];
    if buf_ptr == 0 || len == 0 {
        return NtStatus::Success;
    }
    let buf_phys = match unsafe { user_ptr_to_phys(buf_ptr) } {
        Some(p) => p,
        None => return NtStatus::InvalidParameter,
    };
    let buf = unsafe { core::slice::from_raw_parts(buf_phys as *const u8, len) };
    match write_file(handle, buf) {
        Ok(_written) => NtStatus::Success,
        Err(status) => status,
    }
}

fn handle_allocate_virtual_memory(args: [usize; 16]) -> NtStatus {
    // Windows x64 ABI: RDI=handle, RSI=BaseAddress pointer, RDX=ZeroBits,
    // R10=RegionSize pointer/value, R8=AllocationType, R9=Protect.
    let size = args[3];
    match allocate_virtual_memory(size) {
        Ok(base) => {
            let base_ptr = args[1] as *mut u64;
            if !base_ptr.is_null() {
                unsafe { base_ptr.write(base); }
            }
            NtStatus::Success
        }
        Err(status) => status,
    }
}

fn handle_free_virtual_memory(args: [usize; 16]) -> NtStatus {
    let base = args[0] as u64;
    let size = args[1];
    free_virtual_memory(base, size)
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

/// Translate a guest pointer to a physical address the kernel can read/write.
///
/// For interpreted threads the page-table root is zero and guest addresses
/// are already physical. For native threads we walk the current process page
/// table.
unsafe fn user_ptr_to_phys(addr: u64) -> Option<u64> {
    if addr == 0 {
        return None;
    }
    let cr3 = crate::win32::scheduler::with_current_thread(|t| t.process_page_table_root)
        .unwrap_or(0);
    if cr3 == 0 {
        return Some(addr);
    }
    let pt = crate::mm::page_table::page_table_root(cr3)?;
    pt.translate(addr)
}

/// Read a null-terminated ASCII string of at most `max` bytes from `phys`.
unsafe fn guest_cstr(phys: u64, max: usize) -> &'static str {
    let mut len = 0usize;
    let bytes = core::slice::from_raw_parts(phys as *const u8, max);
    while len < max && bytes[len] != 0 {
        len += 1;
    }
    core::str::from_utf8_unchecked(&bytes[..len])
}
