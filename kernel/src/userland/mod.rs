pub mod brew;
pub mod shell;
pub mod compositor;

pub fn init() {
    log::info!("userland: initializing");
    brew::init();
    compositor::init();
    shell::init();
}

pub fn run_shell() -> ! {
    shell::Shell::run();
}
