//! PC Speaker driver (x86_64) — PIT-driven beeps.
//!
//! Uses Intel 8254 Programmable Interval Timer channel 2 to drive
//! the PC speaker for basic audio output. Supports square-wave
//! beeps at configurable frequency and duration.

#[cfg(target_arch = "x86_64")]
use core::arch::asm;

/// PIT clock frequency (Hz).
const PIT_FREQUENCY: u32 = 1_193_182;

/// I/O port addresses for PIT and PC speaker.
#[cfg(target_arch = "x86_64")]
const PIT_CHANNEL2: u16 = 0x42;
#[cfg(target_arch = "x86_64")]
const PIT_COMMAND: u16 = 0x43;
#[cfg(target_arch = "x86_64")]
const PC_SPEAKER: u16 = 0x61;

/// Initialize the PC speaker driver.
pub fn init() {
    #[cfg(target_arch = "x86_64")]
    {
        log::info!("pcspkr: initializing PC speaker driver");
        // Start with speaker silenced
        silence();
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        log::info!("pcspkr: PC speaker not available on this architecture");
    }
}

/// Play a beep at the given frequency (Hz) for the given duration (ms).
///
/// On x86_64, this programs PIT channel 2 and connects it to the PC speaker.
/// Duration is approximate — the caller should call `silence()` after the
/// desired time has elapsed (the kernel timer or scheduler can handle this).
#[cfg(target_arch = "x86_64")]
pub fn beep(freq: u32, _duration_ms: u32) {
    if freq == 0 || freq > 20000 {
        return;
    }

    let divisor = (PIT_FREQUENCY / freq) as u16;

    unsafe {
        // Program PIT channel 2: mode 3 (square wave), lobyte/hibyte
        outb(PIT_COMMAND, 0xB6);
        // Set frequency divisor
        outb(PIT_CHANNEL2, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL2, ((divisor >> 8) & 0xFF) as u8);

        // Connect PIT channel 2 to PC speaker:
        // Read current state of port 0x61, set bits 0 and 1
        let mut val = inb(PC_SPEAKER);
        val |= 0x03; // Set bit 0 (gate) and bit 1 (speaker)
        outb(PC_SPEAKER, val);
    }
}

/// Silence the PC speaker.
#[cfg(target_arch = "x86_64")]
pub fn silence() {
    unsafe {
        let mut val = inb(PC_SPEAKER);
        val &= !0x03; // Clear bit 0 (gate) and bit 1 (speaker data)
        outb(PC_SPEAKER, val);
    }
}

/// No-op on non-x86_64 architectures.
#[cfg(not(target_arch = "x86_64"))]
pub fn beep(_freq: u32, _duration_ms: u32) {}

#[cfg(not(target_arch = "x86_64"))]
pub fn silence() {}

/// Read a byte from an I/O port.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!(
        "in al, dx",
        out("al") val,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    val
}

/// Write a byte to an I/O port.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn outb(port: u16, val: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nomem, nostack, preserves_flags)
    );
}