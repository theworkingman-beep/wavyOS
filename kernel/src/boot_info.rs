//! Bootloader-independent boot information types.
//!
//! These types mirror the fields we need from `bootloader_api` so that the
//! kernel can be built for architectures that do not use the `bootloader`
//! crate.

/// Physical memory region reported by the bootloader/firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: MemoryRegionKind,
}

impl Default for MemoryRegion {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            kind: MemoryRegionKind::Reserved,
        }
    }
}

/// Kind of memory region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRegionKind {
    Usable,
    Reserved,
    Bootloader,
    Unknown,
}

/// Pixel format of the framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    Bgr,
    U8,
    Unknown {
        red_position: u8,
        green_position: u8,
        blue_position: u8,
    },
}

/// Framebuffer metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameBufferInfo {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bytes_per_pixel: usize,
    pub pixel_format: PixelFormat,
}

#[cfg(feature = "arch_x86_64")]
impl From<bootloader_api::info::MemoryRegionKind> for MemoryRegionKind {
    fn from(kind: bootloader_api::info::MemoryRegionKind) -> Self {
        match kind {
            bootloader_api::info::MemoryRegionKind::Usable => MemoryRegionKind::Usable,
            bootloader_api::info::MemoryRegionKind::Bootloader => MemoryRegionKind::Bootloader,
            bootloader_api::info::MemoryRegionKind::UnknownUefi(_) => MemoryRegionKind::Unknown,
            _ => MemoryRegionKind::Reserved,
        }
    }
}

#[cfg(feature = "arch_x86_64")]
impl From<bootloader_api::info::MemoryRegion> for MemoryRegion {
    fn from(region: bootloader_api::info::MemoryRegion) -> Self {
        Self {
            start: region.start,
            end: region.end,
            kind: region.kind.into(),
        }
    }
}

#[cfg(feature = "arch_x86_64")]
impl From<bootloader_api::info::PixelFormat> for PixelFormat {
    fn from(fmt: bootloader_api::info::PixelFormat) -> Self {
        match fmt {
            bootloader_api::info::PixelFormat::Rgb => PixelFormat::Rgb,
            bootloader_api::info::PixelFormat::Bgr => PixelFormat::Bgr,
            bootloader_api::info::PixelFormat::U8 => PixelFormat::U8,
            bootloader_api::info::PixelFormat::Unknown {
                red_position,
                green_position,
                blue_position,
            } => PixelFormat::Unknown {
                red_position,
                green_position,
                blue_position,
            },
            _ => PixelFormat::Rgb,
        }
    }
}

#[cfg(feature = "arch_x86_64")]
impl From<bootloader_api::info::FrameBufferInfo> for FrameBufferInfo {
    fn from(info: bootloader_api::info::FrameBufferInfo) -> Self {
        Self {
            width: info.width,
            height: info.height,
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            pixel_format: info.pixel_format.into(),
        }
    }
}
