//! Kernel-space shell that can operate standalone (UART) or via PTY
//! When connected to a PTY (slave output goes to PTY master), output is
//! directed through the PTY instead of just UART logging.

use alloc::string::String;
use alloc::vec::Vec;
use crate::input::{self, InputEvent};

/// Global PTY ID that this shell instance is connected to (0 = standalone/UART mode)
static mut SHELL_PTY_ID: usize = 0;

pub struct Shell {
    line: String,
    parts: Vec<String>,
}

impl Shell {
    pub fn new() -> Self {
        Self { line: String::new(), parts: Vec::new() }
    }

    pub fn run() -> ! {
        let mut sh = Self::new();
        loop {
            sh.prompt();
            sh.readline();
            sh.exec();
        }
    }

    /// Write output - goes to PTY if connected, otherwise UART log
    fn shell_write(&self, s: &str) {
        let pty_id = unsafe { SHELL_PTY_ID };
        if pty_id != 0 {
            crate::pty::pty_slave_write(pty_id, s.as_bytes());
        } else {
            log::info!("{}", s);
        }
    }

    fn prompt(&self) {
        self.shell_write("vibe-sh> ");
    }

    fn readline(&mut self) {
        self.line.clear();
        let pty_id = unsafe { SHELL_PTY_ID };
        loop {
            // If connected to a PTY, read from PTY slave (master->slave direction)
            if pty_id != 0 {
                let mut buf = [0u8; 64];
                let n = crate::pty::pty_slave_read(pty_id, &mut buf);
                for i in 0..n {
                    let ascii = buf[i];
                    match ascii {
                        b'\n' => {
                            self.shell_write("\n");
                            break;
                        }
                        8 | 127 => {
                            if self.line.pop().is_some() {
                                self.shell_write("\x08 \x08"); // backspace, space, backspace
                            }
                        }
                        ascii if ascii >= 32 && ascii < 127 => {
                            self.line.push(ascii as char);
                            // Echo the character back
                            let echobuf = [ascii];
                            self.shell_write(core::str::from_utf8(&echobuf).unwrap_or("?"));
                        }
                        _ => {}
                    }
                }
                if n == 0 {
                    crate::scheduler::yield_cpu();
                    continue;
                }
                if self.line.contains('\n') || buf.iter().take(n).any(|&b| b == b'\n') {
                    break;
                }
            } else {
                // Standalone mode: read from kernel input subsystem
                if let Some(event) = input::poll() {
                    match event {
                        InputEvent::KeyPress { ascii } => {
                            match ascii {
                                b'\n' => break,
                                b'\x08' => { self.line.pop(); }
                                ascii if ascii >= 32 && ascii < 127 => {
                                    self.line.push(ascii as char);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                } else {
                    crate::scheduler::yield_cpu();
                }
            }
        }
        self.parts.clear();
        for p in self.line.split_whitespace() {
            self.parts.push(String::from(p));
        }
    }

    fn exec(&self) {
        if self.parts.is_empty() { return; }
        let parts: Vec<&str> = self.parts.iter().map(|s| s.as_str()).collect();
        match parts[0] {
            "help" => self.shell_write("commands: help brew ls ps whoami exit\n"),
            "ls" => self.shell_write("bin dev etc lib usr\n"),
            "ps" => self.shell_write("PID 1 init\n"),
            "whoami" => self.shell_write("root\n"),
            "brew" => self.brew_cmd(&parts[1..]),
            "exit" => self.shell_write("shell exiting\n"),
            _ => {
                let mut msg = String::from("unknown command: ");
                msg.push_str(parts[0]);
                msg.push('\n');
                self.shell_write(&msg);
            }
        }
    }

    fn brew_cmd(&self, args: &[&str]) {
        if args.is_empty() {
            self.shell_write("brew.sh: package manager for Vibe Coded OS\n");
            return;
        }
        match args[0] {
            "install" => {
                if args.len() < 2 { self.shell_write("brew install <pkg>\n"); return; }
                let _ = super::brew::install(args[1]);
            }
            "search" => {
                if args.len() < 2 { self.shell_write("brew search <query>\n"); return; }
                let hits = super::brew::search(args[1]);
                for h in hits { 
                    let mut line = String::from("  ");
                    line.push_str(&h);
                    line.push('\n');
                    self.shell_write(&line);
                }
            }
            "list" => {
                let pkgs = super::brew::list_installed();
                for p in pkgs {
                    let mut line = String::from("  ");
                    line.push_str(&p);
                    line.push('\n');
                    self.shell_write(&line);
                }
            }
            "repo" => {
                let repos = super::brew::list_repos();
                for (name, url) in repos {
                    let mut line = String::from("  ");
                    line.push_str(&name);
                    line.push_str(": ");
                    line.push_str(&url);
                    line.push('\n');
                    self.shell_write(&line);
                }
            }
            _ => {
                let mut msg = String::from("brew: unknown subcommand '");
                msg.push_str(args[0]);
                msg.push_str("'\n");
                self.shell_write(&msg);
            }
        }
    }
}

/// Set the PTY ID for the current shell (called before Shell::run)
pub fn set_pty_id(id: usize) {
    unsafe { SHELL_PTY_ID = id; }
}

pub fn init() {
    log::info!("userland: initializing brew.sh, shell, compositor");
    super::brew::init();
    super::compositor::init();
}

pub fn run_shell() -> ! {
    Shell::run();
}