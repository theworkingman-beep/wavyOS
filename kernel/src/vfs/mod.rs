//! Minimal in-memory virtual filesystem.
//!
//! Provides the backing store for NT file syscalls and native kernel paths.
//! A real implementation will use a disk filesystem and page cache.

use crate::mm::frame_allocator;
use core::cmp::min;
use spin::Mutex;

const MAX_NAME_LEN: usize = 64;
const MAX_NODES: usize = 256;
const MAX_OPEN_FILES: usize = 64;
const FILE_DATA_FRAMES: usize = 16; // 64 KiB max per file for early bring-up

/// Node type in the VFS tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
}

/// A VFS node.
#[derive(Clone, Copy)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: usize,
}

impl Node {
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHandle(pub usize);

/// Open file state.
pub struct OpenFile {
    pub node: NodeId,
    pub position: usize,
    pub writable: bool,
}

static NODES: Mutex<[Option<Node>; MAX_NODES]> = Mutex::new([const { None }; MAX_NODES]);
static FILE_DATA: Mutex<[[Option<u64>; FILE_DATA_FRAMES]; MAX_NODES]> =
    Mutex::new([[None; FILE_DATA_FRAMES]; MAX_NODES]);
static FILE_SIZES: Mutex<[usize; MAX_NODES]> = Mutex::new([0; MAX_NODES]);
static OPEN_FILES: Mutex<[Option<OpenFile>; MAX_OPEN_FILES]> =
    Mutex::new([const { None }; MAX_OPEN_FILES]);

pub fn init() {
    // Create root directory at node 0.
    let mut nodes = NODES.lock();
    nodes[0] = Some(Node {
        kind: NodeKind::Directory,
        parent: None,
        name: [0; MAX_NAME_LEN],
        name_len: 1,
    });
    nodes[0].as_mut().unwrap().name[0] = b'/';
}

/// Lookup a child node by name.
fn find_child(parent: NodeId, name: &str) -> Option<NodeId> {
    let nodes = NODES.lock();
    for (i, slot) in nodes.iter().enumerate() {
        if let Some(node) = slot {
            if node.parent == Some(parent) && node.name_str() == name {
                return Some(NodeId(i));
            }
        }
    }
    None
}

/// Create a node under `parent`.
pub fn create(parent: NodeId, name: &str, kind: NodeKind) -> Option<NodeId> {
    if name.len() > MAX_NAME_LEN {
        return None;
    }
    if find_child(parent, name).is_some() {
        return None;
    }

    let mut nodes = NODES.lock();
    let index = nodes.iter().position(|s| s.is_none())?;
    let mut name_buf = [0u8; MAX_NAME_LEN];
    name_buf[..name.len()].copy_from_slice(name.as_bytes());
    nodes[index] = Some(Node {
        kind,
        parent: Some(parent),
        name: name_buf,
        name_len: name.len(),
    });
    Some(NodeId(index))
}

/// Lookup a path from the root.
pub fn lookup(path: &str) -> Option<NodeId> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Some(NodeId(0));
    }
    let mut current = NodeId(0);
    for component in path.split('/') {
        if component.is_empty() {
            continue;
        }
        current = find_child(current, component)?;
    }
    Some(current)
}

/// Open a node as a file.
pub fn open(node: NodeId, writable: bool) -> Option<FileHandle> {
    {
        let nodes = NODES.lock();
        if !matches!(nodes[node.0]?.kind, NodeKind::File) {
            return None;
        }
    }
    let mut open = OPEN_FILES.lock();
    let index = open.iter().position(|s| s.is_none())?;
    open[index] = Some(OpenFile {
        node,
        position: 0,
        writable,
    });
    Some(FileHandle(index))
}

/// Return the current size of the file identified by `handle`.
pub fn file_size(handle: FileHandle) -> Option<usize> {
    let open = OPEN_FILES.lock();
    let file = open[handle.0].as_ref()?;
    let sizes = FILE_SIZES.lock();
    Some(sizes[file.node.0])
}

/// Close an open file handle.
pub fn close(handle: FileHandle) -> bool {
    let mut open = OPEN_FILES.lock();
    if open[handle.0].is_some() {
        open[handle.0] = None;
        true
    } else {
        false
    }
}

/// Read up to `buf.len()` bytes from `handle` into `buf`.
pub fn read(handle: FileHandle, buf: &mut [u8]) -> Option<usize> {
    let mut open = OPEN_FILES.lock();
    let file = open[handle.0].as_mut()?;
    let node = file.node.0;

    let sizes = FILE_SIZES.lock();
    let size = sizes[node];
    drop(sizes);

    let mut read = 0usize;
    while read < buf.len() && file.position < size {
        let frame_index = file.position / 4096;
        let frame_offset = file.position % 4096;
        let to_read = min(buf.len() - read, min(4096 - frame_offset, size - file.position));

        let data = FILE_DATA.lock();
        if let Some(frame) = data[node][frame_index] {
            let src = (frame + frame_offset as u64) as *const u8;
            unsafe {
                core::ptr::copy_nonoverlapping(src, buf[read..read + to_read].as_mut_ptr(), to_read);
            }
        } else {
            // Sparse unallocated frame reads as zeros.
            buf[read..read + to_read].fill(0);
        }
        drop(data);

        file.position += to_read;
        read += to_read;
    }
    Some(read)
}

/// Write `buf` to `handle`.
pub fn write(handle: FileHandle, buf: &[u8]) -> Option<usize> {
    let mut open = OPEN_FILES.lock();
    let file = open[handle.0].as_mut()?;
    if !file.writable {
        return None;
    }
    let node = file.node.0;

    let mut written = 0usize;
    while written < buf.len() {
        let frame_index = file.position / 4096;
        if frame_index >= FILE_DATA_FRAMES {
            break;
        }
        let frame_offset = file.position % 4096;
        let to_write = min(buf.len() - written, 4096 - frame_offset);

        let mut data = FILE_DATA.lock();
        if data[node][frame_index].is_none() {
            data[node][frame_index] = frame_allocator::allocate();
        }
        let frame = data[node][frame_index].as_mut()?;
        let dst = (*frame + frame_offset as u64) as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(buf[written..written + to_write].as_ptr(), dst, to_write);
        }
        drop(data);

        file.position += to_write;
        written += to_write;

        let mut sizes = FILE_SIZES.lock();
        if file.position > sizes[node] {
            sizes[node] = file.position;
        }
    }
    Some(written)
}
