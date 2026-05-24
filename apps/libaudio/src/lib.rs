//! libaudio — User-space audio API for VibeOS.
//!
//! Provides a high-level interface to the kernel audio subsystem:
//! - Beep generation (PC speaker)
//! - PCM sample playback (16-bit signed, mono, 8kHz)
//! - Volume control
//!
//! All calls delegate to the kernel audio syscalls via libvibe.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

/// Audio format constants (must match kernel).
pub const SAMPLE_RATE: u32 = 8000;
pub const CHANNELS: u8 = 1;
pub const BITS_PER_SAMPLE: u8 = 16;

/// Generate a sine wave at the given frequency and duration.
/// Returns PCM samples (16-bit signed, mono, 8kHz).
pub fn sine_wave(freq: u32, duration_ms: u32) -> Vec<i16> {
    let num_samples = (SAMPLE_RATE as u64 * duration_ms as u64 / 1000) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    // Use a simple sine approximation via a parabolic wave (no libm needed).
    // This uses integer arithmetic with a phase accumulator.
    // Phase is in 0..2^32 representing 0..2*pi.
    let phase_increment = ((freq as u64 * 0x1_0000_0000_u64) / SAMPLE_RATE as u64) as u32;
    let mut phase: u32 = 0;
    for _ in 0..num_samples {
        // Approximate sin using a parabolic approximation (good enough for audio)
        let sample = sine_approx(phase);
        // Scale to ~75% amplitude
        samples.push((sample as i32 * 24000 / 32767) as i16);
        phase = phase.wrapping_add(phase_increment);
    }
    samples
}

/// Simple sine approximation using parabolic wave (Bhaskara I's approximation).
/// Input: phase as u32 where full range = 2*pi.
/// Output: value in range -32767..32767.
fn sine_approx(phase: u32) -> i16 {
    // Convert phase to quadrant-based sine approximation
    let quadrant = (phase >> 30) & 0x3; // 0-3
    let index = phase >> 16; // 0-65535 within quadrant

    // Parabolic approximation within first quadrant
    let x = index as i32; // 0..65535

    // Sine approximation: sin ≈ x * (65535 - x) * 2 / 65535
    let first_half = (2 * x * (65535 - x)) / 65535; // parabola peak at 32767

    // Map to output based on quadrant
    match quadrant {
        0 => {
            if first_half > 32767 { 32767i16 }
            else { first_half as i16 }
        }
        1 => {
            let v = 65535 - first_half;
            if v > 32767 { 32767i16 }
            else { v as i16 }
        }
        2 => {
            if first_half > 32767 { -32767i16 }
            else { (-first_half) as i16 }
        }
        _ => {
            let v = 65535 - first_half;
            if v > 32767 { -32767i16 }
            else { (-v) as i16 }
        }
    }
}

/// Generate a square wave at the given frequency and duration.
/// Returns PCM samples (16-bit signed, mono, 8kHz).
pub fn square_wave(freq: u32, duration_ms: u32) -> Vec<i16> {
    let num_samples = (SAMPLE_RATE as u64 * duration_ms as u64 / 1000) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    let period_samples = SAMPLE_RATE as f64 / freq as f64;
    for i in 0..num_samples {
        let phase = (i as f64 % period_samples) / period_samples;
        if phase < 0.5 {
            samples.push(20000);
        } else {
            samples.push(-20000);
        }
    }
    samples
}

/// Generate white noise for the given duration.
/// Returns PCM samples (16-bit signed, mono, 8kHz).
pub fn noise(duration_ms: u32) -> Vec<i16> {
    let num_samples = (SAMPLE_RATE as u64 * duration_ms as u64 / 1000) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    // Simple LFSR-based pseudo-random noise
    let mut lfsr: u32 = 0xACE1u32;
    for _ in 0..num_samples {
        let bit = ((lfsr >> 0) ^ (lfsr >> 2) ^ (lfsr >> 3) ^ (lfsr >> 5)) & 1;
        lfsr = (lfsr >> 1) | (bit << 15);
        let sample = if bit == 1 { 16000i16 } else { -16000i16 };
        samples.push(sample);
    }
    samples
}

/// Play a beep at the given frequency and duration (in ms).
/// This uses the kernel PC speaker driver directly.
pub fn beep(freq: u32, duration_ms: u32) {
    vibe::audio_beep(freq, duration_ms);
}

/// Play PCM samples through the audio output buffer.
/// Samples must be 16-bit signed, mono, 8kHz.
/// Returns the number of bytes written.
pub fn play(samples: &[i16]) -> usize {
    let byte_slice = unsafe {
        core::slice::from_raw_parts(
            samples.as_ptr() as *const u8,
            samples.len() * 2,
        )
    };
    vibe::audio_pcm_write(byte_slice)
}

/// Set the master volume (0-255, where 128 is typical).
pub fn set_volume(vol: u8) {
    vibe::audio_set_volume(vol);
}

/// Get the current master volume (0-255).
pub fn get_volume() -> u8 {
    vibe::audio_get_volume()
}

/// Stop all audio playback and clear buffers.
pub fn stop() {
    vibe::audio_stop();
}

/// Mix two mono PCM streams by adding samples with clipping.
/// Both streams must be the same length.
pub fn mix(a: &[i16], b: &[i16]) -> Vec<i16> {
    let len = a.len().min(b.len());
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        // Add with saturation (clipping)
        let mixed = (a[i] as i32 + b[i] as i32);
        result.push(if mixed > 32767 { 32767 } else if mixed < -32768 { -32768 } else { mixed as i16 });
    }
    result
}