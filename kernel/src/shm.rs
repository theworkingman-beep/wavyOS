use alloc::vec::Vec;
use spin::Mutex;

pub struct ShmRegion {
    pub id: usize,
    pub phys_start: usize,
    pub size: usize,
    pub refs: usize,
}

static REGIONS: Mutex<Vec<ShmRegion>> = Mutex::new(Vec::new());
static NEXT_ID: Mutex<usize> = Mutex::new(1);

pub fn init() {}

pub fn create(size: usize) -> Option<usize> {
    let layout = core::alloc::Layout::from_size_align(size, 4096).ok()?;
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() { return None; }
    let id = { let mut n = NEXT_ID.lock(); let v = *n; *n += 1; v };
    REGIONS.lock().push(ShmRegion { id, phys_start: ptr as usize, size, refs: 1 });
    Some(id)
}

pub fn lookup(id: usize) -> Option<(usize, usize)> {
    for r in REGIONS.lock().iter() {
        if r.id == id {
            return Some((r.phys_start, r.size));
        }
    }
    None
}
