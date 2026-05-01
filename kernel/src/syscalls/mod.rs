//! Syscall dispatch table
use core::arch::asm;

pub fn init() {
    // Set up syscall/sysret or SVC handler
}

pub enum Syscall {
    Exit = 0,
    Write = 1,
    Read = 2,
    Spawn = 3,
    Yield = 4,
    MachOExec = 0x700, // macOS compatibility entry
}

pub unsafe fn dispatch(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    match n {
        0 => { /* exit */ 0 }
        1 => { /* write */ a2 }
        2 => { /* read */ 0 }
        0x700 => {
            // Mach-O exec entry
            crate::compat::macho::exec(a1 as *const u8, a2 as usize)
        }
        _ => 0,
    }
}
