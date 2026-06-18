//! Memory management.
//!
//! Provides a frame allocator backed by the bootloader memory map and a bump
//! allocator for early kernel heap use. A proper page allocator will replace
//! the bump allocator once the MMU is configured per-architecture.

pub mod frame_allocator;
pub mod page_table;

use crate::boot_info::{MemoryRegion, MemoryRegionKind};
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

/// Global bump allocator for early allocations.
static BUMP_ALLOC: Mutex<BumpAllocator> = Mutex::new(BumpAllocator::new());
static TOTAL_FRAMES: AtomicUsize = AtomicUsize::new(0);

pub fn init() {
    // The bootloader memory map is read during kernel_main and passed in here.
    // For now, the allocator is initialized lazily on first allocation.
}

/// Initialize the early heap using a contiguous physical memory region.
pub fn init_heap(start: u64, end: u64) {
    let mut bump = BUMP_ALLOC.lock();
    bump.base = start;
    bump.next = start;
    bump.limit = end;
}

struct BumpAllocator {
    base: u64,
    next: u64,
    limit: u64,
}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            base: 0,
            next: 0,
            limit: 0,
        }
    }

    fn allocate(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let aligned = (self.next + (align as u64 - 1)) & !(align as u64 - 1);
        let end = aligned.checked_add(size as u64)?;
        if end > self.limit {
            return None;
        }
        self.next = end;
        Some(aligned as *mut u8)
    }
}

/// Allocate `size` bytes with `align` alignment from the early heap.
pub fn alloc_early(size: usize, align: usize) -> Option<*mut u8> {
    BUMP_ALLOC.lock().allocate(size, align)
}

/// Count usable frames in the bootloader memory map.
pub fn count_usable_frames(regions: &[MemoryRegion]) -> usize {
    let mut count = 0;
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            count += (region.end - region.start) / 4096;
        }
    }
    TOTAL_FRAMES.store(count as usize, Ordering::Relaxed);
    count as usize
}

/// Return the number of usable 4 KiB frames.
pub fn total_frames() -> usize {
    TOTAL_FRAMES.load(Ordering::Relaxed)
}

/// Initialize physical frame allocation from the bootloader memory map.
///
/// # Safety
/// The memory map must describe the real physical memory layout.
pub unsafe fn init_physical_allocator(regions: &[MemoryRegion]) {
    frame_allocator::init(regions);
}
