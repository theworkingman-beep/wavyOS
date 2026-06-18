//! In-memory registry shim.
//!
//! Aperture OS provides the HKCU/HKLM hives expected by Win32. Values are
//! stored in a simple flat table until a persistent registry store is implemented.

use spin::Mutex;

/// Registry value types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegValueType {
    Dword = 4,
    Qword = 11,
    String = 1,
    Binary = 3,
}

/// A registry value.
#[derive(Clone, Copy)]
pub struct RegValue {
    pub ty: RegValueType,
    pub data: [u8; 256],
    pub len: usize,
}

const MAX_KEYS: usize = 256;

static REGISTRY: Mutex<[Option<(&'static str, RegValue)>; MAX_KEYS]> =
    Mutex::new([None; MAX_KEYS]);

pub fn init() {
    // Default hive values can be pre-seeded here.
}

/// Set a value in the in-memory registry.
pub fn set_value(key: &str, value: &str, ty: RegValueType, data: &[u8]) -> bool {
    let mut path = [0u8; 256];
    let combined = format_path(key, value, &mut path);
    let leaked = leak_str(combined);

    let mut reg = REGISTRY.lock();
    let mut value_data = [0u8; 256];
    let len = data.len().min(256);
    value_data[..len].copy_from_slice(&data[..len]);

    let entry = RegValue { ty, data: value_data, len };

    if let Some(slot) = reg.iter_mut().find(|s| matches!(s, Some((k, _)) if *k == leaked)) {
        *slot = Some((leaked, entry));
        true
    } else if let Some(slot) = reg.iter_mut().find(|s| s.is_none()) {
        *slot = Some((leaked, entry));
        true
    } else {
        false
    }
}

/// Get a value from the in-memory registry.
pub fn get_value(key: &str, value: &str) -> Option<RegValue> {
    let mut path = [0u8; 256];
    let combined = format_path(key, value, &mut path);
    let reg = REGISTRY.lock();
    reg.iter().find_map(|s| {
        if let Some((k, v)) = s {
            if *k == combined {
                return Some(*v);
            }
        }
        None
    })
}

fn format_path<'a>(key: &str, value: &str, buf: &'a mut [u8; 256]) -> &'a str {
    let mut len = 0usize;
    for b in key.bytes() {
        if len >= buf.len() {
            break;
        }
        buf[len] = b;
        len += 1;
    }
    if len < buf.len() {
        buf[len] = b'\\';
        len += 1;
    }
    for b in value.bytes() {
        if len >= buf.len() {
            break;
        }
        buf[len] = b;
        len += 1;
    }
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

fn leak_str(s: &str) -> &'static str {
    // For early bring-up we intentionally leak a copy to satisfy 'static.
    // A real implementation uses a registry heap allocator.
    let copy = crate::mm::alloc_early(s.len(), 1).expect("registry key alloc");
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), copy as *mut u8, s.len());
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(copy as *const u8, s.len()))
    }
}
