use alloc::string::String;
use alloc::vec::Vec;
use crate::input::{self, InputEvent};

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

    fn prompt(&self) {
        log::info!("vibe-sh>");
    }

    fn readline(&mut self) {
        self.line.clear();
        loop {
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
        self.parts.clear();
        for p in self.line.split_whitespace() {
            self.parts.push(String::from(p));
        }
    }

    fn exec(&self) {
        if self.parts.is_empty() { return; }
        let parts: Vec<&str> = self.parts.iter().map(|s| s.as_str()).collect();
        match parts[0] {
            "help" => log::info!("commands: help brew ls ps whoami exit"),
            "ls" => log::info!("bin dev etc lib usr"),
            "ps" => log::info!("PID 1 init"),
            "whoami" => log::info!("root"),
            "brew" => self.brew_cmd(&parts[1..]),
            "exit" => log::info!("shell exiting"),
            _ => log::warn!("unknown command: {}", parts[0]),
        }
    }

    fn brew_cmd(&self, args: &[&str]) {
        if args.is_empty() {
            log::info!("brew.sh: package manager for Vibe Coded OS");
            return;
        }
        match args[0] {
            "install" => {
                if args.len() < 2 { log::warn!("brew install <pkg>"); return; }
                let _ = super::brew::install(args[1]);
            }
            "search" => {
                if args.len() < 2 { log::warn!("brew search <query>"); return; }
                let hits = super::brew::search(args[1]);
                for h in hits { log::info!("  {}", h); }
            }
            "list" => {
                let pkgs = super::brew::list_installed();
                for p in pkgs { log::info!("  {}", p); }
            }
            "repo" => {
                let repos = super::brew::list_repos();
                for (name, url) in repos { log::info!("  {}: {}", name, url); }
            }
            _ => log::warn!("brew: unknown subcommand '{}'", args[0]),
        }
    }
}

pub fn init() {
    log::info!("userland: initializing brew.sh, shell, compositor");
    super::brew::init();
    super::compositor::init();
}

pub fn run_shell() -> ! {
    Shell::run();
}
