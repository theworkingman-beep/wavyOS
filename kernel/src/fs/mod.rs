use spin::Mutex;
use alloc::string::String;
use alloc::vec::Vec;

pub mod fat32;
pub mod virtio_blk;

pub const MAX_PATH_LEN: usize = 256;
pub const MAX_OPEN_FILES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: u32,
}

pub trait FileSystem: Send + Sync {
    fn open(&self, path: &str) -> Result<FileHandle, FsError>;
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError>;
    fn read(&self, handle: &FileHandle, offset: u32, buf: &mut [u8]) -> Result<usize, FsError>;
}

pub struct FileHandle {
    pub ino: u32,
    pub size: u32,
    pub path: String,
}

#[derive(Debug)]
pub enum FsError {
    NotFound,
    InvalidPath,
    IoError,
    NotADirectory,
    NotAFile,
    NoSpace,
}

struct VfsState {
    fs: Option<&'static dyn FileSystem>,
}

static VFS: Mutex<VfsState> = Mutex::new(VfsState { fs: None });

pub fn register_fs(fs: &'static dyn FileSystem) {
    VFS.lock().fs = Some(fs);
    log::info!("vfs: filesystem registered");
}

pub fn read_file(path: &str) -> Result<Vec<u8>, FsError> {
    let state = VFS.lock();
    let fs = state.fs.ok_or(FsError::NotFound)?;
    drop(state);

    let handle = fs.open(path)?;
    let mut buf = alloc::vec![0u8; handle.size as usize];
    let _ = fs.read(&handle, 0, &mut buf)?;
    Ok(buf)
}

pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, FsError> {
    let state = VFS.lock();
    let fs = state.fs.ok_or(FsError::NotFound)?;
    fs.read_dir(path)
}

pub fn init() {
    log::info!("vfs: initialized");
    log::info!("vfs: no block device found, filesystem unavailable");
}
