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

    // Kernel higher-half PML4 entry index. The bootloader identity-maps the
    // kernel at low addresses, but we also keep the kernel visible in every
    // process by sharing the highest PML4 entry (entry 511).
    /// Physical address of the kernel's top-level page table, captured once
    /// during early initialization.
    static mut KERNEL_CR3: u64 = 0;

    /// Capture the currently active top-level page table.
    ///
    /// # Safety
    /// Must be called exactly once during early boot while the bootloader's
    /// page tables are still active.
    pub unsafe fn capture_kernel_page_table() {
        let value: u64;
        core::arch::asm!("mov {0}, cr3", out(reg) value);
        KERNEL_CR3 = value;
    }

    /// A top-level page table for the 4-level x86_64 MMU.
    pub struct PageTable {
        pub root: u64,
    }

    impl PageTable {
        /// Allocate and zero a new top-level page table, sharing the kernel's
        /// higher-half mapping so that system calls can access kernel code and
        /// data without switching CR3.
        pub fn new() -> Option<Self> {
            let root = frame_allocator::allocate()?;
            unsafe {
                core::ptr::write_bytes(root as *mut u8, 0, 4096);
            }

            // Copy the entire kernel page table into the new top-level table.
            // This keeps the kernel mapped in every process while allowing the
            // lower half to be replaced with per-process user mappings. A real
            // implementation will switch to a proper higher-half shared entry.
            if unsafe { KERNEL_CR3 } != 0 {
                unsafe {
                    let src = KERNEL_CR3 as *const u64;
                    let dst = root as *mut u64;
                    for i in 0..512 {
                        dst.add(i).write(src.add(i).read());
                    }
                }
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

        /// Map `pages` contiguous physical frames starting at `phys_base` to
        /// `virt_base`, using `flags` for each 4 KiB page.
        ///
        /// # Safety
        /// The virtual range must not already be mapped.
        pub unsafe fn map_region(&mut self, virt_base: u64, phys_base: u64, pages: usize, flags: u64) -> bool {
            for i in 0..pages as u64 {
                if !self.map(virt_base + i * 4096, phys_base + i * 4096, flags) {
                    return false;
                }
            }
            true
        }

        /// Return the physical address that should be loaded into CR3.
        pub fn cr3(&self) -> u64 {
            self.root
        }

        /// Translate a virtual address through this page table.
        ///
        /// Returns the physical address (including page offset) or `None` if
        /// the virtual address is not mapped.
        pub fn translate(&self, virt: u64) -> Option<u64> {
            let pml4_index = ((virt >> 39) & 0x1FF) as usize;
            let pdpt_index = ((virt >> 30) & 0x1FF) as usize;
            let pd_index = ((virt >> 21) & 0x1FF) as usize;
            let pt_index = ((virt >> 12) & 0x1FF) as usize;

            let pdpt = Self::next_table(self.root, pml4_index, false)?;
            let pd = Self::next_table(pdpt, pdpt_index, false)?;
            let pt = Self::next_table(pd, pd_index, false)?;

            let entries = pt as *const u64;
            let entry = unsafe { entries.add(pt_index).read() };
            if (entry & PAGE_PRESENT) == 0 {
                return None;
            }
            Some((entry & !0xFFF) | (virt & 0xFFF))
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
pub use x86_64_impl::{capture_kernel_page_table, PageTable};

/// Return a mutable reference to the page table rooted at physical address
/// `cr3`. This is a debug/utility helper that treats the physical address as
/// identity-mapped.
///
/// # Safety
/// `cr3` must be a valid, identity-mapped top-level page table.
pub unsafe fn page_table_root(cr3: u64) -> Option<PageTable> {
    if cr3 == 0 {
        return None;
    }
    Some(PageTable { root: cr3 })
}

#[cfg(feature = "arch_aarch64")]
pub struct PageTable {
    pub root: u64,
}

#[cfg(feature = "arch_aarch64")]
impl PageTable {
    pub fn new() -> Option<Self> {
        None
    }

    pub unsafe fn map(
        &mut self,
        _virt: u64,
        _phys: u64,
        _flags: u64,
    ) -> bool {
        false
    }

    pub unsafe fn map_region(
        &mut self,
        _virt: u64,
        _phys: u64,
        _pages: usize,
        _flags: u64,
    ) -> bool {
        false
    }

    pub fn cr3(&self) -> u64 {
        self.root
    }

    pub fn translate(&self,
        _virt: u64,
    ) -> Option<u64> {
        None
    }
}

/// Common page-table flags.
pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITABLE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_EXECUTE: u64 = 1 << 63; // NX bit when EFER.NXE is set.
