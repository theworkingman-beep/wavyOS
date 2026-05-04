// Window manager types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    None,
    Window(u64),
    Taskbar,
}
