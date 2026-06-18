//! NT object-manager-style handle table.
//!
//! A lightweight handle allocator used by the Win32 subsystem to reference
//! processes, threads, files, registry keys, and window stations.

use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

const MAX_HANDLES: usize = 1024;

pub fn init() {
    // Handle table is zero-initialized by static storage.
}

/// Kinds of kernel objects that can be referenced by a handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Process,
    Thread,
    File,
    RegistryKey,
    WindowStation,
    Desktop,
}

/// An object header stored in the handle table.
#[derive(Clone, Copy)]
pub struct ObjectHeader {
    pub kind: ObjectKind,
    pub data: usize,
}

impl ObjectHeader {
    /// Return the stored pointer.
    pub fn ptr<T>(self) -> *mut T {
        self.data as *mut T
    }
}

unsafe impl Send for ObjectHeader {}
unsafe impl Sync for ObjectHeader {}

static HANDLE_TABLE: Mutex<[Option<ObjectHeader>; MAX_HANDLES]> = Mutex::new([const { None }; MAX_HANDLES]);
static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);

/// Allocate a handle for `object`.
pub fn allocate(kind: ObjectKind, data: *mut ()) -> Option<Handle> {
    let mut table = HANDLE_TABLE.lock();
    let index = table.iter().position(|slot| slot.is_none())?;
    table[index] = Some(ObjectHeader { kind, data: data as usize });
    Some(Handle(NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)))
}

/// Look up an object by handle.
pub fn lookup(handle: Handle) -> Option<ObjectHeader> {
    let table = HANDLE_TABLE.lock();
    let index = (handle.0 as usize).wrapping_sub(1) % MAX_HANDLES;
    table[index]
}

/// Close a handle.
pub fn close(handle: Handle) -> bool {
    let mut table = HANDLE_TABLE.lock();
    let index = (handle.0 as usize).wrapping_sub(1) % MAX_HANDLES;
    if table[index].is_some() {
        table[index] = None;
        true
    } else {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle(pub u32);
