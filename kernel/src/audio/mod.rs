//! Audio subsystem — PCM playback, mixer, PC speaker.
//!
//! Provides kernel-side audio support:
//! - PC speaker beeper (x86_64 PIT-driven)
//! - PCM sample buffer for user-space audio output
//! - Volume control
//! - Audio device abstraction

use spin::Mutex;
use alloc::vec::Vec;

/// Audio sample format: 16-bit signed PCM, mono, 8000 Hz.
/// This is the baseline format; the mixer can upsample if needed.
pub const AUDIO_SAMPLE_RATE: u32 = 8000;
pub const AUDIO_CHANNELS: u8 = 1;
pub const AUDIO_BITS_PER_SAMPLE: u8 = 16;

/// Maximum PCM buffer size in bytes (1 second of audio at 8kHz mono 16-bit)
pub const PCM_BUF_SIZE: usize = AUDIO_SAMPLE_RATE as usize * (AUDIO_BITS_PER_SAMPLE as usize / 8) * AUDIO_CHANNELS as usize;

/// Audio command sent via IPC or syscall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioCmd {
    /// Play a beep at the given frequency (Hz) and duration (ms).
    /// payload[0..4] = frequency (u32 LE), payload[4..8] = duration_ms (u32 LE)
    Beep = 1,
    /// Write PCM samples to the output buffer.
    /// payload[0..4] = length of PCM data that follows, payload[4..] = PCM data
    PcmWrite = 2,
    /// Set master volume (0-255).
    /// payload[0] = volume level
    SetVolume = 3,
    /// Get master volume.
    GetVolume = 4,
    /// Stop all audio playback.
    Stop = 5,
}

/// Master volume level (0-255).
static MASTER_VOLUME: Mutex<u8> = Mutex::new(128);

/// PCM output ring buffer.
static PCM_BUFFER: Mutex<PcmRingBuffer> = Mutex::new(PcmRingBuffer::new());

/// State of the audio subsystem.
static AUDIO_INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Simple ring buffer for PCM samples.
struct PcmRingBuffer {
    data: Vec<u8>,
    head: usize,
    tail: usize,
}

impl PcmRingBuffer {
    const fn new() -> Self {
        Self {
            data: Vec::new(),
            head: 0,
            tail: 0,
        }
    }

    fn write(&mut self, samples: &[u8]) -> usize {
        for &byte in samples {
            if self.data.len() < PCM_BUF_SIZE {
                self.data.push(byte);
            } else {
                // Ring buffer: overwrite oldest
                self.data[self.head] = byte;
                self.head = (self.head + 1) % PCM_BUF_SIZE;
                if self.head == self.tail {
                    self.tail = (self.tail + 1) % PCM_BUF_SIZE;
                }
            }
        }
        samples.len()
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if self.head != self.tail && self.tail < self.data.len() {
                *byte = self.data[self.tail];
                self.tail = (self.tail + 1) % self.data.len();
                count += 1;
            } else {
                *byte = 0; // silence
                count += 1;
            }
        }
        count
    }

    fn len(&self) -> usize {
        if self.head >= self.tail {
            self.head - self.tail
        } else {
            self.data.len() - self.tail + self.head
        }
    }

    fn clear(&mut self) {
        self.data.clear();
        self.head = 0;
        self.tail = 0;
    }
}

/// Initialize the audio subsystem.
pub fn init() {
    log::info!("audio: initializing subsystem ({}Hz, {}ch, {}bit)",
        AUDIO_SAMPLE_RATE, AUDIO_CHANNELS, AUDIO_BITS_PER_SAMPLE);

    #[cfg(target_arch = "x86_64")]
    {
        crate::drivers::pcspkr::init();
        log::info!("audio: PC speaker driver initialized");
    }

    *AUDIO_INITIALIZED.lock() = true;
    log::info!("audio: subsystem ready");
}

/// Play a beep through the PC speaker (x86_64 only).
pub fn beep(freq: u32, duration_ms: u32) {
    #[cfg(target_arch = "x86_64")]
    {
        crate::drivers::pcspkr::beep(freq, duration_ms);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (freq, duration_ms);
        log::debug!("audio: beep not supported on this architecture");
    }
}

/// Write PCM samples to the output buffer.
/// Returns the number of bytes written.
pub fn pcm_write(samples: &[u8]) -> usize {
    let volume = *MASTER_VOLUME.lock();
    let mut buf = PCM_BUFFER.lock();

    // Apply volume scaling (simple linear attenuation)
    if volume == 0 {
        return 0; // muted
    }

    if volume >= 255 {
        // No scaling needed
        buf.write(samples)
    } else {
        // Scale samples by volume (16-bit signed PCM)
        let mut scaled = alloc::vec::Vec::with_capacity(samples.len());
        let vol_ratio = volume as f32 / 255.0;
        let chunks = samples.chunks_exact(2);
        for chunk in chunks {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            let scaled_sample = (sample as f32 * vol_ratio) as i16;
            scaled.extend_from_slice(&scaled_sample.to_le_bytes());
        }
        // Handle remaining byte if odd length
        if samples.len() % 2 != 0 {
            scaled.push(samples[samples.len() - 1]);
        }
        buf.write(&scaled)
    }
}

/// Read PCM samples from the buffer (for the DAC/driver to consume).
/// Fills the buffer with silence if not enough samples are available.
pub fn pcm_read(buf: &mut [u8]) -> usize {
    PCM_BUFFER.lock().read(buf)
}

/// Set the master volume (0-255).
pub fn set_volume(vol: u8) {
    *MASTER_VOLUME.lock() = vol;
}

/// Get the master volume (0-255).
pub fn get_volume() -> u8 {
    *MASTER_VOLUME.lock()
}

/// Stop all audio (clear PCM buffer, silence speaker).
pub fn stop() {
    PCM_BUFFER.lock().clear();

    #[cfg(target_arch = "x86_64")]
    {
        crate::drivers::pcspkr::silence();
    }
}

/// Check if the audio subsystem is initialized.
pub fn is_initialized() -> bool {
    *AUDIO_INITIALIZED.lock()
}
