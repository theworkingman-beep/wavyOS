use std::path::Path;

fn main() {
    let ws_path = Path::new("../target/vibeos-x86_64/release/windowserver");
    let shell_path = Path::new("../target/vibeos-x86_64/release/desktop_shell");
    let terminal_path = Path::new("../target/vibeos-x86_64/release/terminal");

    let ws_exists = ws_path.exists();
    let shell_exists = shell_path.exists();
    let terminal_exists = terminal_path.exists();

    if ws_exists && shell_exists {
        println!("cargo:rustc-cfg=feature=\"userspace_gui\"");
        println!("cargo:rerun-if-changed={}", ws_path.display());
        println!("cargo:rerun-if-changed={}", shell_path.display());
    }

    if terminal_exists {
        println!("cargo:rustc-cfg=feature=\"userspace_terminal\"");
        println!("cargo:rerun-if-changed={}", terminal_path.display());
    }
}