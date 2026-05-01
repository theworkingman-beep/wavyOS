//! Dynamic linker (dyld) stub for Mach-O binaries
///
/// When a Mach-O binary is marked dynamic (has LC_LOAD_DYLINKER or LC_LOAD_DYLIB),
/// the kernel would normally invoke /usr/lib/dyld. Since we don't have a real
/// user-space dyld yet, this stub resolves a minimal set of symbols and
/// prepares a fake "executable" environment.

use alloc::string::String;
use alloc::vec::Vec;

/// Minimal dyld resolution state
pub struct DyldState {
    pub libs: Vec<String>,
    pub entry: u64,
    pub base: u64,
}

impl DyldState {
    pub fn new(entry: u64, base: u64) -> Self {
        Self {
            libs: Vec::new(),
            entry,
            base,
        }
    }
}

/// Stub: "load" a requested dylib by name. In a real OS this would
/// open the shared library file, parse its Mach-O, and resolve exports.
pub fn load_dylib(name: &str, state: &mut DyldState) -> bool {
    crate::log::info!("dyld: loading stub library '{}'", name);
    state.libs.push(String::from(name));
    true
}

/// Stub: resolve a symbol. Returns a fake address for well-known
/// Foundation / libc symbols so simple "hello world" style binaries
/// don't immediately page-fault.
pub fn resolve_symbol(_lib: &str, name: &str) -> Option<u64> {
    match name {
        "_printf" | "_NSLog" | "_malloc" | "_free" | "_pthread_create"
        | "_strlen" | "_memcpy" | "_memset" | "_exit" => {
            // Return a sentinel address in the "stub trampoline" region
            Some(0xFFFF_0000_0000_0000 | name.as_bytes()[0] as u64)
        }
        _ => None,
    }
}

/// Called by the Mach-O exec path when a dynamic binary is detected.
pub fn run_dyld_stub(entry: u64, base: u64, _bind_data: &[u8]) {
    let mut state = DyldState::new(entry, base);
    // In a real implementation we would iterate LC_LOAD_DYLIB commands here.
    crate::log::info!(
        "dyld stub: preparing dynamic binary entry={:#x} base={:#x}",
        state.entry, state.base
    );
    // TODO: build argv/envp/auxv, jump to entry point
    let _ = &mut state;
}
