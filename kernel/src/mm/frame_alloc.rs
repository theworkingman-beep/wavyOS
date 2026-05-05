//! Physical frame allocator — bitmap-based
use crate::MemoryRegion;
use spin::Mutex;

pub const PAGE_SIZE: usize = 4096;

/// Bitmap-based physical frame allocator
pub struct FrameAllocator {
    bitmap: &'static mut [u64],
    total_frames: usize,
    used_frames: usize,
    base_frame: usize, // first frame index covered by bitmap
}

impl FrameAllocator {
    pub const fn new() -> Mutex<Option<Self>> {
        Mutex::new(None)
    }

    /// Initialize the allocator. Returns the number of available frames.
    pub unsafe fn init(memory_map: &[MemoryRegion]) -> usize {
        // Calculate total usable frames
        let mut total_usable = 0usize;
        let mut max_phys = 0u64;
        for region in memory_map {
            if matches!(region.kind, crate::MemoryRegionKind::Usable) {
                total_usable += (region.length / PAGE_SIZE as u64) as usize;
                let end = region.base + region.length;
                if end > max_phys {
                    max_phys = end;
                }
            }
        }

        let total_frames = (max_phys / PAGE_SIZE as u64) as usize;
        let bitmap_bits = total_frames;
        let bitmap_words = (bitmap_bits + 63) / 64;

        // Allocate bitmap from heap (already initialized by mm::init)
        let bitmap_layout = alloc::alloc::Layout::from_size_align(
            bitmap_words * 8,
            8,
        ).unwrap();
        let bitmap_ptr = alloc::alloc::alloc(bitmap_layout);
        if bitmap_ptr.is_null() {
            panic!("FrameAllocator: failed to allocate bitmap");
        }
        core::ptr::write_bytes(bitmap_ptr, 0xFF, bitmap_words * 8); // all used initially
        let bitmap = core::slice::from_raw_parts_mut(bitmap_ptr as *mut u64, bitmap_words);

        // Mark usable regions as free
        for region in memory_map {
            if !matches!(region.kind, crate::MemoryRegionKind::Usable) {
                continue;
            }
            let start_frame = (region.base / PAGE_SIZE as u64) as usize;
            let num_frames = (region.length / PAGE_SIZE as u64) as usize;
            for i in 0..num_frames {
                let frame = start_frame + i;
                if frame < total_frames {
                    let word = frame / 64;
                    let bit = frame % 64;
                    bitmap[word] &= !(1u64 << bit);
                }
            }
        }

        let alloc = FrameAllocator {
            bitmap,
            total_frames,
            used_frames: 0,
            base_frame: 0,
        };
        // Count free frames
        let free = alloc.count_free();

        *FRAME_ALLOCATOR.lock() = Some(alloc);

        free
    }

    fn count_free(&self) -> usize {
        let mut free = 0;
        for word in self.bitmap.iter() {
            free += word.count_ones() as usize;
        }
        free
    }

    /// Allocate a single physical frame, returns its physical address
    pub fn alloc_frame() -> Option<usize> {
        let mut guard = FRAME_ALLOCATOR.lock();
        let alloc = guard.as_mut()?;
        for (i, word) in alloc.bitmap.iter_mut().enumerate() {
            if *word != !0u64 {
                let bit = word.trailing_ones() as usize;
                *word |= 1u64 << bit;
                let frame = i * 64 + bit;
                alloc.used_frames += 1;
                return Some(frame * PAGE_SIZE);
            }
        }
        None
    }

    /// Allocate `count` contiguous frames, returns the first frame's physical address
    pub fn alloc_frames(count: usize) -> Option<usize> {
        let mut guard = FRAME_ALLOCATOR.lock();
        let alloc = guard.as_mut()?;

        let mut consecutive = 0;
        let mut start_frame = None;

        for (i, word) in alloc.bitmap.iter_mut().enumerate() {
            if *word == !0u64 {
                consecutive = 0;
                start_frame = None;
                continue;
            }
            for bit in 0..64 {
                let frame = i * 64 + bit;
                if frame >= alloc.total_frames {
                    break;
                }
                let is_free = (*word & (1u64 << bit)) == 0;
                if is_free {
                    if start_frame.is_none() {
                        start_frame = Some(frame);
                    }
                    consecutive += 1;
                    if consecutive >= count {
                        // Mark all frames as used
                        for j in 0..count {
                            let f = start_frame.unwrap() + j;
                            let w = f / 64;
                            let b = f % 64;
                            alloc.bitmap[w] |= 1u64 << b;
                        }
                        alloc.used_frames += count;
                        return Some(start_frame.unwrap() * PAGE_SIZE);
                    }
                } else {
                    consecutive = 0;
                    start_frame = None;
                }
            }
        }
        None
    }

    /// Free a physical frame
    pub fn free_frame(phys_addr: usize) {
        let mut guard = FRAME_ALLOCATOR.lock();
        let alloc = guard.as_mut().unwrap();
        let frame = phys_addr / PAGE_SIZE;
        if frame < alloc.total_frames {
            let word = frame / 64;
            let bit = frame % 64;
            alloc.bitmap[word] &= !(1u64 << bit);
            alloc.used_frames -= 1;
        }
    }

    /// Free `count` contiguous frames starting at phys_addr
    pub fn free_frames(phys_addr: usize, count: usize) {
        for i in 0..count {
            Self::free_frame(phys_addr + i * PAGE_SIZE);
        }
    }
}

static FRAME_ALLOCATOR: Mutex<Option<FrameAllocator>> = FrameAllocator::new();

pub fn init(memory_map: &[MemoryRegion]) -> usize {
    unsafe { FrameAllocator::init(memory_map) }
}

pub fn alloc_frame() -> Option<usize> {
    FrameAllocator::alloc_frame()
}

pub fn alloc_frames(count: usize) -> Option<usize> {
    FrameAllocator::alloc_frames(count)
}

pub fn free_frame(phys_addr: usize) {
    FrameAllocator::free_frame(phys_addr)
}

pub fn free_frames(phys_addr: usize, count: usize) {
    FrameAllocator::free_frames(phys_addr, count)
}
