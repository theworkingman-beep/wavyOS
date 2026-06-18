//! Filesystem shim for Windows binaries.
//!
//! Maps Win32 paths such as `C:\Windows\System32` to Aperture OS VFS paths.
//! The actual filesystem implementation lives in the kernel VFS; this module
//! only performs path normalization.

const MAX_PATH: usize = 260;

/// Normalize a Win32 path to an Aperture OS absolute path.
pub fn normalize(path: &str) -> Option<&str> {
    if path.len() > MAX_PATH {
        return None;
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        // Strip drive letter and map to /windows or root.
        Some(&path[2..])
    } else {
        Some(path)
    }
}
