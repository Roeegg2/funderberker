use core::{
    arch::asm,
    arch::x86_64::__cpuid,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use page_size::MAX_BOTTOM_PAGING_LEVEL;
use pat::{PatType, setup_pat};
use pmm::PmmAllocator;
use utils::{
    mem::{PhysAddr, VirtAddr},
    sanity_assert,
};

#[cfg(feature = "limine")]
use limine::memory_map::{self, EntryType};
use utils::mem::memset;

use crate::mem::paging::{Flags, PageSize, PagingError};

use super::{
    X86_64,
    cpu::{
        Cr3, Cr4, Register,
        msr::{AmdMsr, Efer, rdmsr, wrmsr},
    },
};

pub mod flags;
pub mod page_size;
pub mod pat;

/// The number of entries per page table
pub const ENTRIES_PER_TABLE: usize = 512;

/// An entry in a page table
#[repr(C)]
#[derive(Debug)]
pub(super) struct Entry(usize);

/// A page table
#[repr(C, align(4096))]
#[derive(Debug)]
pub(super) struct PageTable([Entry; ENTRIES_PER_TABLE]);

#[allow(dead_code)]
impl Entry {
    #[inline]
    #[must_use]
    const fn get_flags(&self) -> Flags<X86_64> {
        unsafe { Flags::<X86_64>::from_raw(self.0 & 0xFFF) }
    }

    /// Sets the flags of the entry to be the given flags.
    ///
    /// NOTE:
    /// This clears whatever flags were previously set, so it should be used with caution.
    #[inline]
    const fn set_flags(&mut self, flags: Flags<X86_64>) {
        // Clear the flags bits
        self.0 &= !0xFFF;

        // Set the new flags
        self.0 |= flags.data();
    }

    /// Returns the entry's address
    #[inline]
    #[must_use]
    const fn get_addr(&self, page_size: PageSize<X86_64>) -> PhysAddr {
        assert!(
            self.0 & !0xfff & page_size.get_offset_mask() == 0,
            "Address is not aligned to the page size"
        );
        // Get the address bits from the entry
        PhysAddr(self.0 & !page_size.get_offset_mask())
    }

    #[inline]
    fn set_addr(&mut self, addr: PhysAddr, page_size: PageSize<X86_64>) {
        assert!(
            addr.0 & page_size.get_offset_mask() == 0,
            "Address is not aligned to the page size"
        );
        // Clear the address bits
        self.0 &= page_size.get_offset_mask();

        // Set the new address
        self.0 |= addr.0 & !page_size.get_offset_mask();
    }

    #[must_use]
    fn next_level_table(&mut self) -> &mut PageTable {
        let ptr: *mut PageTable = core::ptr::without_provenance_mut(
            self.get_addr(PageSize::size_4kb()).add_hhdm_offset().0,
        );

        unsafe {
            ptr.cast::<PageTable>()
                .as_mut()
                .expect("Failed to get next level table")
        }
    }

    /// Immediately maps the entry to the given physical address with the given flags.
    unsafe fn map(
        &mut self,
        phys_addr: PhysAddr,
        flags: Flags<X86_64>,
        page_size: PageSize<X86_64>,
    ) -> Result<(), PagingError> {
        if self.get_flags().get_present() {
            return Err(PagingError::PageAlreadyPresent);
        }

        self.set_addr(phys_addr, page_size);
        self.set_flags(unsafe {
            flags
                .set_present(true)
                .set_last_entry(true)
                .join(page_size.into())
                .ok_or(PagingError::InvalidFlags)?
        });

        Ok(())
    }

    /// Marks the entry as not present and frees the physical page if the entry was activated not
    /// manually (ie. activated using a call to `activate`).
    fn release(&mut self, page_size: PageSize<X86_64>) -> Result<(), PagingError> {
        // XXX: need to determine page size here for freeing
        let flags = self.get_flags();
        if !self.get_flags().get_present() {
            return Err(PagingError::PageNotPresent);
        }

        if flags.get_allocated() {
            let phys_addr = self.get_addr(page_size);
            unsafe {
                pmm::get().free(phys_addr, 1).expect("Failed to free page");
            }
        }

        self.set_flags(flags.set_present(false));

        Ok(())
    }
}

impl PageTable {
    /// Allocates a new page table
    pub fn new() -> (&'static mut Self, PhysAddr) {
        let phys_addr = pmm::get()
            .allocate(PageSize::size_4kb().page_alignment(), 1)
            .expect("Failed to allocate page table");

        // For easier bootstrapping, we are HHDM mapping all page tables
        let ptr: *mut u8 = core::ptr::without_provenance_mut(phys_addr.add_hhdm_offset().0);
        // Memset to clear old stale data that might be in the page tables
        unsafe {
            memset(ptr, 0, size_of::<PageTable>());
        };

        (
            unsafe { ptr.cast::<PageTable>().as_mut().unwrap() },
            phys_addr,
        )
    }

    /// Tries to get a reference to the `Entry` associated with the given virtual address.
    ///
    /// If the entry is not present, `None` is returned.
    #[must_use]
    fn get_entry(&mut self, virt_addr: VirtAddr) -> Option<(&mut Entry, PageSize<X86_64>)> {
        let mut table = self;

        // In contrast to the `get_*_table_range` methods, here we want to return the actual table
        // mapping the entry, not one level above it, so we translate until `page_size.bottom_paging_level()`
        for level in (0..=MAX_BOTTOM_PAGING_LEVEL).rev() {
            let i = next_level_index(virt_addr, level);

            let flags = table[i].get_flags();
            if flags.get_last_entry() {
                let page_size = PageSize::from_bottom_paging_level(level)?;
                return Some((&mut table[i], page_size));
            } else if flags.get_present() {
                table = table[i].next_level_table();
            } else {
                return None;
            }
        }

        unreachable!()
    }

    /// Gets the parent page table of the given `base_addr`.
    ///
    /// If one of the page tables are missing during translation, a new page table is created.
    #[must_use]
    fn get_create_table_range(
        &mut self,
        base_addr: VirtAddr,
        page_size: PageSize<X86_64>,
    ) -> &mut PageTable {
        sanity_assert!(
            base_addr.0 % page_size.size() == 0,
            "Address is not aligned"
        );

        // A page table entry addresses are stored in descending order: (|PML5|PML4|PDPT|PDE|PTE|offset|)
        //
        // We start from the highest level, and go down. In our case we want to return the last
        // level table, not the last entry, so we translate until `page_size.bottom_paging_level() + 1` (inclusive)
        let mut table = self;
        for level in (page_size.bottom_paging_level() + 1..=MAX_BOTTOM_PAGING_LEVEL).rev() {
            let i = next_level_index(base_addr, level);
            let flags = table[i].get_flags();
            if !flags.get_present() {
                table[i].set_flags(flags.set_present(true).set_read_write(true));
                let (_, phys_addr) = PageTable::new();
                table[i].set_addr(phys_addr, PageSize::size_4kb());
            }

            table = table[i].next_level_table();
        }

        table
    }

    /// Tries to get the parent table of the given `base_addr`.
    ///
    /// If one of the page tables are missing during the translation, `None` is returned
    #[must_use]
    fn get_table_range(
        &mut self,
        base_addr: VirtAddr,
        page_size: PageSize<X86_64>,
    ) -> Option<&mut PageTable> {
        sanity_assert!(
            base_addr.0 % page_size.size() == 0,
            "Address is not aligned"
        );

        // A page table entry addresses are stored in descending order: (|PML5|PML4|PDPT|PDE|PTE|offset|)
        //
        // We start from the highest level, and go down. In our case we want to return the last
        // level table, not the last entry, so we translate until `page_size.bottom_paging_level() + 1` (inclusive)
        let mut table = self;
        for level in (page_size.bottom_paging_level() + 1..=MAX_BOTTOM_PAGING_LEVEL).rev() {
            let i = next_level_index(base_addr, level);
            if !table[i].get_flags().get_present() {
                return None;
            }

            table = table[i].next_level_table();
        }

        Some(table)
    }

    /// Maps the given virtual address to the given physical address
    pub unsafe fn map_pages(
        &mut self,
        base_addr: VirtAddr,
        phys_addr: PhysAddr,
        page_count: usize,
        page_size: PageSize<X86_64>,
        flags: Flags<X86_64>,
    ) -> Result<(), PagingError> {
        if base_addr.0 % page_size.size() != 0 {
            return Err(PagingError::InvalidVirtualAddress);
        } else if phys_addr.0 % page_size.size() != 0 {
            return Err(PagingError::InvalidPhysicalAddress);
        }

        // Get the parent page table
        let table = self.get_create_table_range(base_addr, page_size);

        // Extract the index to the entry
        let to_skip = next_level_index(base_addr, page_size.bottom_paging_level());
        if to_skip + page_count > ENTRIES_PER_TABLE {
            return Err(PagingError::BadPageCountAndAddressCombination);
        }

        for (i, entry) in table.iter_mut().skip(to_skip).take(page_count).enumerate() {
            unsafe {
                entry.map(phys_addr + (i * page_size.size()), flags, page_size)?;
            };
        }

        Ok(())
    }

    /// Unmaps the given virtual address range
    pub(super) unsafe fn unmap_pages(
        &mut self,
        base_addr: VirtAddr,
        page_count: usize,
        page_size: PageSize<X86_64>,
    ) -> Result<(), PagingError> {
        if base_addr.0 % page_size.size() != 0 {
            return Err(PagingError::InvalidVirtualAddress);
        }

        let table = self
            .get_table_range(base_addr, page_size)
            .ok_or(PagingError::PageNotPresent)?;

        let to_skip = next_level_index(base_addr, page_size.bottom_paging_level());
        if to_skip + page_count > ENTRIES_PER_TABLE {
            return Err(PagingError::BadPageCountAndAddressCombination);
        }

        for entry in table.iter_mut().skip(to_skip).take(page_count) {
            entry.release(page_size)?;
        }

        Ok(())
    }

    /// Get the physical address associated with the given virtual address.
    ///
    /// If the virtual address is not mapped, `None` is returned.
    #[must_use]
    pub(super) fn translate(&mut self, base_addr: VirtAddr) -> Option<PhysAddr> {
        let (entry, page_size) = self.get_entry(base_addr)?;

        let flags = entry.get_flags();
        if !flags.get_present() {
            return None;
        }

        Some(entry.get_addr(page_size))
    }
}

/// Get the top level paging table PML4/PML5 (depending on the paging level)
pub(super) fn get_pml() -> &'static mut PageTable {
    let phys_addr = unsafe { PhysAddr((Cr3::read().top_pml() << 12) as usize) };

    let ptr: *mut PageTable = core::ptr::without_provenance_mut(phys_addr.add_hhdm_offset().0);

    unsafe { ptr.cast::<PageTable>().as_mut().expect("Failed to get PML") }
}

#[inline]
#[must_use]
const fn next_level_index(addr: VirtAddr, level: usize) -> usize {
    (addr.0 >> (PageSize::size_4kb().offset_bit_count() + (level * 9))) & 0b1_1111_1111
}

/// Helper function to avoid code duplication.
///
/// Maps in the given virtual address to the given
fn map_in_entry(
    mut base_virt_addr: VirtAddr,
    mut base_phys_addr: PhysAddr,
    mut total_size: usize,
    new_pml: &mut PageTable,
    mut flags: Flags<X86_64>,
    pat: Option<PatType>,
) {
    // Breaking into pages and allocating
    while total_size != 0 {
        let page_size = if total_size >= PageSize::size_1gb().size()
            && base_phys_addr.0 % PageSize::size_1gb().size() == 0
            && base_virt_addr.0 % PageSize::size_1gb().size() == 0
        {
            PageSize::size_1gb()
        } else if total_size >= PageSize::size_2mb().size()
            && base_phys_addr.0 % PageSize::size_2mb().size() == 0
            && base_virt_addr.0 % PageSize::size_2mb().size() == 0
        {
            PageSize::size_2mb()
        } else if total_size >= PageSize::size_4kb().size()
            && base_phys_addr.0 % PageSize::size_4kb().size() == 0
            && base_virt_addr.0 % PageSize::size_4kb().size() == 0
        {
            PageSize::size_4kb()
        } else {
            unreachable!()
        };

        // PAT flags depend on the page size, so we do this here
        if let Some(pat_type) = pat {
            flags = flags.set_pat(pat_type, page_size);
        }

        unsafe {
            new_pml
                .map_pages(base_virt_addr, base_phys_addr, 1, page_size, flags)
                .unwrap()
        };

        total_size -= page_size.size();
        base_phys_addr.0 += page_size.size();
        base_virt_addr.0 += page_size.size();
    }
}

#[cfg(feature = "limine")]
pub(super) unsafe fn init_from_limine(
    mem_map: &[&memory_map::Entry],
    kernel_virt: VirtAddr,
    kernel_phys: PhysAddr,
    used_by_pmm: &memory_map::Entry,
) {
    // TODO: CPUID check as well
    // #[cfg(feature = "paging_5")]
    // if read_cr!(cr4) & (1 << 12) != 0 {
    //     panic!("5 level paging requested, but not supported");
    // }

    let (new_pml, new_pml_addr) = PageTable::new();

    map_in_entry(
        PhysAddr(used_by_pmm.base as usize).add_hhdm_offset(),
        PhysAddr(used_by_pmm.base as usize),
        used_by_pmm.length as usize,
        new_pml,
        Flags::new().set_read_write(true),
        None,
    );

    // NOTE: We are doing the `EXECUTABLE_AND_MODULES` mapping independently of the other sections,
    // since the kernel's view of this section is different than HHDM (even though this memory is
    // also HHDM mapped IIRC)
    for entry in mem_map
        .iter()
        .filter(|entry| entry.base != used_by_pmm.base)
    {
        match entry.entry_type {
            EntryType::EXECUTABLE_AND_MODULES => map_in_entry(
                kernel_virt,
                kernel_phys,
                entry.length as usize,
                new_pml,
                Flags::new().set_read_write(true),
                None,
            ),
            EntryType::ACPI_RECLAIMABLE | EntryType::BOOTLOADER_RECLAIMABLE | EntryType::USABLE => {
                map_in_entry(
                    PhysAddr(entry.base as usize).add_hhdm_offset(),
                    PhysAddr(entry.base as usize),
                    entry.length as usize,
                    new_pml,
                    Flags::new().set_read_write(true),
                    None,
                )
            }
            #[cfg(feature = "framebuffer")]
            EntryType::FRAMEBUFFER => map_in_entry(
                PhysAddr(entry.base as usize).add_hhdm_offset(),
                PhysAddr(entry.base as usize),
                entry.length as usize,
                new_pml,
                Flags::new().set_read_write(true),
                Some(PatType::WriteCombining),
            ),
            _ => (),
        }
    }

    unsafe { finalize_init(new_pml_addr) };

    logger::info!("Paging system initialized successfully");
}

/// Check if the CPU supports Paging Global Enable (PGE).
#[inline]
fn check_pge_support() {
    const PGE_BIT: u32 = 1 << 12;

    unsafe {
        // TODO: possibly handle the case this isn't supported?
        assert!(
            __cpuid(1).edx & PGE_BIT != 0,
            "Paging Global Enable (PGE) is not supported by the CPU"
        );
    }
}

/// Check if the CPU supports NX (No-Execute) bit.
#[inline]
fn check_nx_support() {
    const NX_BIT: u32 = 1 << 20;

    unsafe {
        assert!(
            __cpuid(0x8000_0001).edx & NX_BIT != 0,
            "No-Execute (NX) bit is not supported by the CPU"
        );
    }
}

#[inline]
fn invlpg(addr: VirtAddr) {
    unsafe {
        asm!(
            "invlpg [{}]",
            in(reg) addr.0,
            options(nostack, nomem, preserves_flags),
        );
    }
}

// TODO: Take care of PCIDs
#[inline]
fn flush_tlb() {
    unsafe {
        asm!(
            "mov {0}, cr3",
            "mov cr3, {0}",
            out(reg) _,
                  options(nostack, preserves_flags),
        )
    }
}

/// Enable the No-Execute (NX) bit in the EFER MSR.
#[inline]
fn enable_nx() {
    check_nx_support();

    let mut efer: u64 = unsafe { rdmsr(AmdMsr::Efer).into() };
    efer |= Efer::NX;
    unsafe { wrmsr(AmdMsr::Efer, efer.into()) };
}

/// Finalize the initialization of the paging system by moving over to the newly setup page table,
/// and enabling the necessary features.
unsafe fn finalize_init(pml_phys_addr: PhysAddr) {
    // TODO: Make sure CRs flags are OK
    unsafe {
        // Check to make sure the features we want to enable are supported
        check_pge_support();
        setup_pat();
        enable_nx();

        // Enable global pages support
        let mut cr4 = Cr4::read();
        cr4.set_pge(1);
        cr4.write();

        // Set the CR3 register to the new PML
        let mut cr3 = Cr3::read();
        cr3.set_top_pml(pml_phys_addr.0 as u64 >> 12);
        cr3.write();
    }
}

impl Deref for PageTable {
    type Target = [Entry; ENTRIES_PER_TABLE];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// possibly TODO:
// PCIDs
// SMEP/SMAP
