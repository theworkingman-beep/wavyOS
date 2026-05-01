//! Mach-O binary loader — expanded with load-command parsing and segment mapping
use alloc::vec::Vec;
use core::mem;

/// Mach-O 64-bit magic
const MH_MAGIC_64: u32 = 0xfeedfacf;
/// CPU types
const CPU_TYPE_ARM64: u32 = 0x0100000c;
const CPU_TYPE_X86_64: u32 = 0x01000007;
/// Load command types
const LC_SEGMENT_64: u32 = 0x19;
const LC_SYMTAB: u32 = 0x02;
const LC_DYSYMTAB: u32 = 0x0b;
const LC_LOAD_DYLINKER: u32 = 0x0e;
const LC_LOAD_DYLIB: u32 = 0x0c;
const LC_UNIXTHREAD: u32 = 0x05;
const LC_MAIN: u32 = 0x28;
const LC_DYLD_INFO: u32 = 0x21;
const LC_DYLD_INFO_ONLY: u32 = 0x22;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LoadCommand {
    cmd: u32,
    cmdsize: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SegmentCommand64 {
    cmd: u32,
    cmdsize: u32,
    segname: [u8; 16],
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    maxprot: u32,
    initprot: u32,
    nsects: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct DyldInfoCommand {
    cmd: u32,
    cmdsize: u32,
    rebase_off: u32,
    rebase_size: u32,
    bind_off: u32,
    bind_size: u32,
    weak_bind_off: u32,
    weak_bind_size: u32,
    lazy_bind_off: u32,
    lazy_bind_size: u32,
    export_off: u32,
    export_size: u32,
}

/// Loaded Mach-O process image
#[derive(Debug)]
pub struct MachOImage {
    pub entry_point: u64,
    pub base_addr: u64,
    pub segments: Vec<(u64, u64, u64, u64)>, // (vmaddr, vmsize, fileoff, filesize)
    pub dynamic: bool,
    pub arch: MachArch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachArch {
    Arm64,
    X86_64,
    Unknown,
}

impl MachOImage {
    pub fn new() -> Self {
        Self {
            entry_point: 0,
            base_addr: u64::MAX,
            segments: Vec::new(),
            dynamic: false,
            arch: MachArch::Unknown,
        }
    }
}

/// Parse a Mach-O binary from raw bytes.
pub fn parse(data: &[u8]) -> Option<MachOImage> {
    if data.len() < mem::size_of::<MachHeader64>() {
        return None;
    }
    let hdr = unsafe { &*(data.as_ptr() as *const MachHeader64) };
    if hdr.magic != MH_MAGIC_64 {
        return None;
    }

    let mut img = MachOImage::new();
    img.arch = match hdr.cputype {
        CPU_TYPE_ARM64 => MachArch::Arm64,
        CPU_TYPE_X86_64 => MachArch::X86_64,
        _ => MachArch::Unknown,
    };

    let mut off = mem::size_of::<MachHeader64>();
    for _ in 0..hdr.ncmds {
        if off + mem::size_of::<LoadCommand>() > data.len() {
            break;
        }
        let lc = unsafe { &*(data.as_ptr().add(off) as *const LoadCommand) };
        match lc.cmd {
            LC_SEGMENT_64 => {
                if off + mem::size_of::<SegmentCommand64>() <= data.len() {
                    let seg = unsafe { &*(data.as_ptr().add(off) as *const SegmentCommand64) };
                    img.segments.push((seg.vmaddr, seg.vmsize, seg.fileoff, seg.filesize));
                    if seg.fileoff > 0 && seg.vmaddr < img.base_addr {
                        img.base_addr = seg.vmaddr;
                    }
                }
            }
            LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
                img.dynamic = true;
            }
            LC_LOAD_DYLINKER | LC_LOAD_DYLIB => {
                img.dynamic = true;
            }
            LC_MAIN => {
                if off + 8 + 8 <= data.len() {
                    let ep = unsafe {
                        let ptr = data.as_ptr().add(off).add(mem::size_of::<LoadCommand>()) as *const u64;
                        ptr.read_unaligned()
                    };
                    img.entry_point = img.base_addr.wrapping_add(ep);
                }
            }
            LC_UNIXTHREAD => {
                // entry point encoded in thread state; simplistic fallback
                // On arm64/x86_64 the PC/RIP is inside the thread command
                // For now rely on LC_MAIN or set a placeholder
            }
            _ => {}
        }
        off += lc.cmdsize as usize;
    }

    crate::log::info!("Mach-O parsed: arch={:?}, entry={:#x}, segments={}", img.arch, img.entry_point, img.segments.len());
    Some(img)
}

/// Minimal exec interface: parse header, map segments, jump placeholder.
pub fn exec(path: *const u8, len: usize) -> usize {
    let data = unsafe { core::slice::from_raw_parts(path, len) };
    let img = match parse(data) {
        Some(i) => i,
        None => return 0xDEAD,
    };

    if img.segments.is_empty() {
        crate::log::warn!("Mach-O has no loadable segments");
        return 0xDEAD;
    }

    crate::log::info!(
        "Mach-O exec ready: arch={:?}, entry={:#x}, dynamic={}",
        img.arch, img.entry_point, img.dynamic
    );

    if img.dynamic {
        crate::log::info!("Dynamic Mach-O — dyld stub would run here");
    }

    // TODO: map segments into user address space, set up stack, transfer control
    0
}
