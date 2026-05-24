//! VirtIO Sound driver for QEMU.
//!
//! Implements audio output via the QEMU virtio-snd-pci device.
//! Currently a stub — full virtqueue-based PCM streaming needs
//! PCI discovery and virtqueue setup (similar to virtio-net).
//!
//! For now, PC speaker provides basic audio. This module will
//! be fully implemented once PCI BAR mapping is available.

/// VirtIO Sound Device ID (PCI).
const VIRTIO_SND_DEVICE_ID: u16 = 0x1009;

/// VirtIO Sound configuration (read from device).
#[repr(C)]
struct VirtioSndConfig {
    /// Number of sound stream CHROMOS (channels?).
    jacks: u8,
    /// Number of stream channels.
    streams: u8,
    /// Number of PCM stream descriptors.
    chmaps: u32,
}

/// VirtIO Sound PCM stream information.
#[repr(C)]
struct VirtioSndPcmInfo {
    hdr: VirtioSndHdr,
    features: u32,
    formats: u64,
    rates: u64,
    channels_min: u8,
    channels_max: u8,
    padding: [u8; 6],
}

/// VirtIO Sound header.
#[repr(C)]
struct VirtioSndHdr {
    code: u32,
    data_len: u32,
}

/// Initialize virtio-snd driver.
pub fn init() {
    log::info!("virtio-snd: initializing");
    // TODO: PCI discovery to find virtio-snd device
    // TODO: Negotiate features (VIRTIO_SND_F_PCM etc.)
    // TODO: Set up virtqueues (control, event, tx, rx)
    // TODO: Configure PCM streams
    log::warn!("virtio-snd: driver stub - PCI discovery not yet implemented");
}

/// Check if a virtio-snd device is present.
pub fn is_available() -> bool {
    // TODO: PCI scan for VIRTIO_VENDOR_ID + VIRTIO_SND_DEVICE_ID
    false
}

/// Write PCM samples to the virtio-snd TX queue.
/// Returns number of bytes written (0 if device not available).
pub fn pcm_write(_samples: &[u8]) -> usize {
    // TODO: Queue PCM samples in the TX virtqueue
    0
}