//! Mach-O binary loader stubs
use alloc::vec::Vec;

/// Minimal Mach-O header constants
const MH_MAGIC_64: u32 = 0xfeedfacf;
const CPU_TYPE_ARM64: u32 = 0x0100000c;
const CPU_TYPE_X86_64: u32 = 0x01000007;

#[repr(C)]
struct MachHeader64 {
    magic: u32,
    cputype: u32,
    cpusubtype: u32,
    filetype: u32,
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    reserved: u32,
}

/// Load and (eventually) execute a Mach-O binary.
/// For now, parses the header and returns a placeholder.
pub fn exec(path: *const u8, _len: usize) -> usize {
    let data = unsafe { core::slice::from_raw_parts(path, _len) };
    if data.len() < core::mem::size_of::<MachHeader64>() {
        return 0;
    }

    let header = unsafe { &*(data.as_ptr() as *const MachHeader64) };
    if header.magic != MH_MAGIC_64 {
        return 0xDEAD;
    }

    let arch = if header.cputype == CPU_TYPE_ARM64 {
        "arm64"
    } else if header.cputype == CPU_TYPE_X86_64 {
        "x86_64"
    } else {
        "unknown"
    };

    crate::log::info!("Mach-O loader: arch={}, ncmds={}", arch, header.ncmds);

    // TODO: parse load commands, map segments, resolve symbols, jump to entry
    0
}
