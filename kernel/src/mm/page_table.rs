//! Architecture-specific page table management.
//!
//! For the x86_64 target this implements a minimal recursive 4-level page
//! table walker. For AArch64 it exposes a stub that can be filled in once the
//! AArch64 MMU bring-up is complete.

#[cfg(feature = "arch_x86_64")]
mod x86_64_impl {
    use crate::mm::frame_allocator;

    const PAGE_PRESENT: u64 = 1 << 0;
    const PAGE_WRITABLE: u64 = 1 << 1;
    const PAGE_USER: u64 = 1 << 2;

    /// A top-level page table for the 4-level x86_64 MMU.
    pub struct PageTable {
        root: u64,
    }

    impl PageTable {
        /// Allocate and zero a new top-level page table.
        pub fn new() -> Option<Self> {
            let root = frame_allocator::allocate()?;
            unsafe {
                core::ptr::write_bytes(root as *mut u8, 0, 4096);
            }
            Some(Self { root })
        }

        /// Map a 4 KiB physical frame to a virtual address with `flags`.
        ///
        /// Missing intermediate page tables are allocated automatically.
        ///
        /// # Safety
        /// The virtual address must not already be mapped through this table and
        /// the caller must ensure the physical frame is valid and owned.
        pub unsafe fn map(&mut self, virt: u64, phys: u64, flags: u64) -> bool {
            let pml4_index = ((virt >> 39) & 0x1FF) as usize;
            let pdpt_index = ((virt >> 30) & 0x1FF) as usize;
            let pd_index = ((virt >> 21) & 0x1FF) as usize;
            let pt_index = ((virt >> 12) & 0x1FF) as usize;

            let pdpt = Self::next_table(self.root, pml4_index, true);
            let Some(pdpt) = pdpt else { return false };
            let pd = Self::next_table(pdpt, pdpt_index, true);
            let Some(pd) = pd else { return false };
            let pt = Self::next_table(pd, pd_index, true);
            let Some(pt) = pt else { return false };

            let entries = pt as *mut u64;
            let entry = entries.add(pt_index);
            entry.write(phys | flags | PAGE_PRESENT);
            true
        }

        /// Return the physical address that should be loaded into CR3.
        pub fn cr3(&self) -> u64 {
            self.root
        }

        /// Return the physical address of the table referenced by `entry` in
        /// `table`, allocating and linking a new table if necessary.
        fn next_table(table: u64, index: usize, create: bool) -> Option<u64> {
            let entries = table as *mut u64;
            unsafe {
                let entry = entries.add(index);
                let value = entry.read();
                if (value & PAGE_PRESENT) != 0 {
                    return Some(value & !0xFFF);
                }
                if !create {
                    return None;
                }
                let frame = frame_allocator::allocate()?;
                core::ptr::write_bytes(frame as *mut u8, 0, 4096);
                entry.write(frame | PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER);
                Some(frame)
            }
        }
    }
}

#[cfg(feature = "arch_x86_64")]
pub use x86_64_impl::PageTable;

#[cfg(feature = "arch_aarch64")]
pub struct PageTable;

#[cfg(feature = "arch_aarch64")]
impl PageTable {
    pub fn new() -> Option<Self> {
        None
    }

    pub unsafe fn map(&mut self, _virt: u64, _phys: u64, _flags: u64) -> bool {
        false
    }

    pub fn cr3(&self) -> u64 {
        0
    }
}

/// Common page-table flags.
pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_EXECUTE: u64 = 1 << 63; // NX bit when EFER.NXE is set.
