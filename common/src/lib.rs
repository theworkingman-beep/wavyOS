#![cfg_attr(not(feature = "std"), no_std)]

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum MemoryRegionKind {
    Usable,
    Reserved,
    Bootloader,
    Kernel,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub kind: MemoryRegionKind,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u8,
}

#[repr(C)]
pub struct BootInfo {
    pub memory_map_ptr: *const MemoryRegion,
    pub memory_map_len: usize,
    pub framebuffer: *const FramebufferInfo,
    pub rsdp: u64,
    pub device_tree: u64,
}
