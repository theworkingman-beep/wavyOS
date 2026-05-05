//! Memory management — frame allocator, heap, and virtual memory
extern crate alloc;

pub mod frame_alloc;

use crate::{MemoryRegion, MemoryRegionKind};
use linked_list_allocator::LockedHeap;
use core::alloc::Layout;

pub use crate::scheduler::STACK_SIZE;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static mut HEAP_START: usize = 0;
static mut HEAP_SIZE: usize = 0;

pub unsafe fn allocate_stack_top() -> usize {
    let layout = Layout::from_size_align(STACK_SIZE, 16).unwrap();
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() { loop { } }
    ptr.add(STACK_SIZE) as usize
}

pub fn init(memory_map: &[MemoryRegion]) {
    // Find the largest usable memory region that doesn't overlap with the kernel
    #[cfg(target_arch = "x86_64")]
    let kernel_start = 0x10000000usize;
    #[cfg(target_arch = "x86_64")]
    let kernel_end = 0x10000000 + 11 * 4096; // ~44 KB for kernel

    #[cfg(target_arch = "aarch64")]
    let kernel_start = 0x40000000usize;
    #[cfg(target_arch = "aarch64")]
    let kernel_end = 0x40000000 + 11 * 4096; // ~44 KB for kernel

    let mut heap_start = 0;
    let mut heap_size = 0;

    for region in memory_map {
        let MemoryRegionKind::Usable = region.kind else { continue };
        let r_start = region.base as usize;
        let r_end = r_start + region.length as usize;

        // Skip regions that overlap with kernel
        if r_start < kernel_end && r_end > kernel_start { continue }

        if region.length as usize > heap_size {
            heap_start = r_start;
            heap_size = region.length as usize;
        }
    }

    // Reserve 1 MB at the start of the heap region for kernel data structures
    if heap_size > 0x100000 {
        heap_start += 0x100000;
        heap_size -= 0x100000;
    }

    log::info!("heap: start={:x}, size={}", heap_start, heap_size);

    unsafe {
        HEAP_START = heap_start;
        HEAP_SIZE = heap_size;
        ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);
    }

    // Initialize physical frame allocator
    let free_frames = frame_alloc::init(memory_map);
    log::info!("physical frames: {} free ({} KB)", free_frames, free_frames * 4);
}
