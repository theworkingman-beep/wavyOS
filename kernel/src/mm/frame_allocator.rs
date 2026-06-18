//! Bitmap-based physical frame allocator.
//!
//! Manages 4 KiB physical frames using a bitset. The bitmap itself is placed
//! at the start of the first usable memory region, and frames occupied by the
//! bitmap are marked reserved.

use crate::boot_info::{MemoryRegion, MemoryRegionKind};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

const FRAME_SIZE: u64 = 4096;
const BITMAP_WORD_BITS: usize = 64;

/// A frame allocator backed by a bitmap.
pub struct FrameAllocator {
    first_frame: u64,
    frame_count: usize,
    bitmap: *mut AtomicU64,
    bitmap_words: usize,
}

unsafe impl Send for FrameAllocator {}
unsafe impl Sync for FrameAllocator {}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            first_frame: 0,
            frame_count: 0,
            bitmap: core::ptr::null_mut(),
            bitmap_words: 0,
        }
    }

    /// Initialize the allocator from a memory map.
    ///
    /// # Safety
    /// `regions` must describe the actual physical memory layout. The bitmap
    /// is written into the first usable region.
    unsafe fn init(&mut self, regions: &[MemoryRegion]) {
        let (usable_start, usable_end, _total_usable) = compute_usable_range(regions);
        let first_frame = align_up(usable_start, FRAME_SIZE);
        let last_frame = align_down(usable_end, FRAME_SIZE);
        let frame_count = ((last_frame - first_frame) / FRAME_SIZE) as usize;

        let bitmap_words = (frame_count + BITMAP_WORD_BITS - 1) / BITMAP_WORD_BITS;
        let bitmap_bytes = bitmap_words * core::mem::size_of::<AtomicU64>();
        let bitmap_addr = align_up(first_frame, core::mem::align_of::<AtomicU64>() as u64);

        // Place bitmap in physical memory and zero it.
        let bitmap = bitmap_addr as *mut AtomicU64;
        for i in 0..bitmap_words {
            core::ptr::write(bitmap.add(i), AtomicU64::new(0));
        }

        self.first_frame = first_frame;
        self.frame_count = frame_count;
        self.bitmap = bitmap;
        self.bitmap_words = bitmap_words;

        // Mark all frames as allocated initially.
        for i in 0..frame_count {
            self.mark(i, true);
        }

        // Free usable frames; keep reserved and bitmap frames allocated.
        for region in regions {
            if region.kind == MemoryRegionKind::Usable {
                let region_first = align_up(region.start, FRAME_SIZE);
                let region_last = align_down(region.end, FRAME_SIZE);
                for frame in (region_first..region_last).step_by(FRAME_SIZE as usize) {
                    if frame >= first_frame {
                        let index = ((frame - first_frame) / FRAME_SIZE) as usize;
                        if index < frame_count && !self.is_bitmap_frame(index) {
                            self.mark(index, false);
                        }
                    }
                }
            }
        }

        // Reserve the frames occupied by the bitmap itself.
        let bitmap_first = (bitmap_addr - first_frame) / FRAME_SIZE;
        let bitmap_last = ((bitmap_addr + bitmap_bytes as u64 - first_frame) / FRAME_SIZE) + 1;
        for i in bitmap_first..bitmap_last.min(frame_count as u64) {
            self.mark(i as usize, true);
        }
    }

    fn is_bitmap_frame(&self, index: usize) -> bool {
        let bitmap_addr = self.bitmap as u64;
        let frame_addr = self.first_frame + index as u64 * FRAME_SIZE;
        frame_addr >= bitmap_addr
            && frame_addr < bitmap_addr + (self.bitmap_words * core::mem::size_of::<AtomicU64>()) as u64
    }

    fn mark(&self, index: usize, used: bool) {
        let word = index / BITMAP_WORD_BITS;
        let bit = index % BITMAP_WORD_BITS;
        let mask = 1u64 << bit;
        let cell = unsafe { &*self.bitmap.add(word) };
        if used {
            let _ = cell.fetch_or(mask, Ordering::Relaxed);
        } else {
            let _ = cell.fetch_and(!mask, Ordering::Relaxed);
        }
    }

    /// Allocate a single 4 KiB physical frame.
    pub fn allocate_frame(&self) -> Option<u64> {
        for word in 0..self.bitmap_words {
            let cell = unsafe { &*self.bitmap.add(word) };
            let mut value = cell.load(Ordering::Relaxed);
            while value != u64::MAX {
                let bit = value.trailing_ones() as usize;
                if bit >= BITMAP_WORD_BITS {
                    break;
                }
                let mask = 1u64 << bit;
                let new_value = value | mask;
                match cell.compare_exchange_weak(
                    value,
                    new_value,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let index = word * BITMAP_WORD_BITS + bit;
                        if index < self.frame_count {
                            return Some(self.first_frame + index as u64 * FRAME_SIZE);
                        }
                        break;
                    }
                    Err(v) => value = v,
                }
            }
        }
        None
    }

    /// Free a previously allocated 4 KiB physical frame.
    pub fn free_frame(&self, frame: u64) {
        if frame < self.first_frame {
            return;
        }
        let index = ((frame - self.first_frame) / FRAME_SIZE) as usize;
        if index >= self.frame_count {
            return;
        }
        self.mark(index, false);
    }

    /// Total number of frames managed by this allocator.
    pub fn total_frames(&self) -> usize {
        self.frame_count
    }
}

static FRAME_ALLOC: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

/// Initialize the frame allocator from a memory map.
///
/// # Safety
/// See [`FrameAllocator::init`].
pub unsafe fn init(regions: &[MemoryRegion]) {
    FRAME_ALLOC.lock().init(regions);
}

/// Allocate a single 4 KiB frame.
pub fn allocate() -> Option<u64> {
    FRAME_ALLOC.lock().allocate_frame()
}

/// Free a 4 KiB frame.
pub fn free(frame: u64) {
    FRAME_ALLOC.lock().free_frame(frame);
}

/// Total number of frames.
pub fn total() -> usize {
    FRAME_ALLOC.lock().total_frames()
}

fn align_up(addr: u64, align: u64) -> u64 {
    (addr + align - 1) & !(align - 1)
}

fn align_down(addr: u64, align: u64) -> u64 {
    addr & !(align - 1)
}

fn compute_usable_range(regions: &[MemoryRegion]) -> (u64, u64, u64) {
    let mut start = u64::MAX;
    let mut end = 0u64;
    let mut total = 0u64;
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            if region.start < start {
                start = region.start;
            }
            if region.end > end {
                end = region.end;
            }
            total += region.end - region.start;
        }
    }
    (start, end, total)
}
