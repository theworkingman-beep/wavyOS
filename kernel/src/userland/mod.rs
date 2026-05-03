pub mod brew;
pub mod shell;
pub mod compositor;
pub mod loader;

pub fn init() {
    log::info!("userland: initializing");
    log::info!("userland: GUI subsystems loaded (WindowServer, Desktop Shell, sample_app on disk image)");
    brew::init();
    compositor::init();
    shell::init();
}

pub fn run_shell() -> ! {
    shell::Shell::run();
}
