//! Simple bump allocator -> linked list allocator
use crate::{BootInfo, MemoryRegion, MemoryRegionKind};
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static mut HEAP_START: usize = 0;
static mut HEAP_SIZE: usize = 0;

pub fn init(memory_map: &[MemoryRegion]) {
    let mut heap_start = 0;
    let mut heap_size = 0;

    for region in memory_map {
        if let MemoryRegionKind::Usable = region.kind {
            if region.length as usize > heap_size {
                heap_start = region.base as usize + 0x100_0000; // offset past kernel
                heap_size = region.length as usize - 0x100_0000;
            }
        }
    }

    unsafe {
        HEAP_START = heap_start;
        HEAP_SIZE = heap_size;
        ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);
    }
}
