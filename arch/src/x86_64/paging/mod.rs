use core::{
    arch::x86_64::__cpuid,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use crate::{
    Arch,
    paging::{Flags, PageSize, PagingError},
    x86_64::cpu::{Cr3, Register},
};
use logger::println;
use page_size::MAX_BOTTOM_PAGING_LEVEL;
use pat::setup_pat;
use pmm::PmmAllocator;
use utils::mem::{PhysAddr, VirtAddr};

#[cfg(feature = "limine")]
use limine::memory_map::{self, EntryType};
use utils::mem::memset;
use utils::sanity_assert;

use super::{
    X86_64,
    cpu::{
        Cr4,
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
    const fn get_flags(&self) -> Flags<X86_64> {
        unsafe { Flags::<X86_64>::from_raw(self.0 & 0xFFF) }
    }

    #[inline]
    const fn set_flags(&mut self, flags: Flags<X86_64>) {
        self.0 |= flags.data();
    }

    /// Returns the entry's physical address
    #[inline]
    const fn get_addr(&self) -> PhysAddr {
        // Possibly mask this address
        PhysAddr(self.0 & !0xFFF)
    }

    #[inline]
    const fn set_addr(&mut self, addr: PhysAddr) {
        self.0 = (self.0 & 0xFFF) | addr.0;
    }

    fn next_level_table(&mut self) -> &mut PageTable {
        let ptr: *mut PageTable = self.get_addr().add_hhdm_offset().into();

        unsafe {
            ptr.cast::<PageTable>()
                .as_mut()
                .expect("Failed to get next level table")
        }
    }

    /// Sets the given flags on and marks the entry as "taken".
    /// The reset of the initializiation will be done later when the entry is activated
    ///
    /// NOTE: As already mentioned, this isn't the same as "present".
    fn take(&mut self, flags: Flags<X86_64>, page_size: PageSize<X86_64>) {
        assert!(!flags.get_taken(), "Entry is already taken");
        assert!(!flags.get_present(), "Entry is already present");

        self.set_flags(flags);
        self.set_flags(page_size.flag());
        // TODO: Remove the present and have activate set it
        self.set_flags(
            Flags::<X86_64>::new()
                .set_taken(true)
                .set_last_entry(true)
                .set_present(true),
        );

        let phys_addr = pmm::get().allocate(1, 1).expect("Failed to allocate page");

        self.set_addr(phys_addr);
        // XXX: Maybe need to memset to 0?
    }

    /// Activates a "taken" entry.
    ///
    /// Most setting up was already done by `take()`, all we need to do now is allocate a physical
    /// page and map the virtual address to it, as well as set the `present` bit.
    fn activate_taken(&mut self) {
        let flags = self.get_flags();
        assert!(flags.get_taken(), "Entry is not taken");
        assert!(!flags.get_present(), "Entry is already present");

        let phys_addr = pmm::get().allocate(1, 1).expect("Failed to allocate page");

        self.set_addr(phys_addr);
        self.set_flags(Flags::<X86_64>::new().set_present(true));
    }

    /// Immediately maps the entry to the given physical address with the given flags.
    unsafe fn map(
        &mut self,
        phys_addr: PhysAddr,
        flags: Flags<X86_64>,
        page_size: PageSize<X86_64>,
    ) {
        assert!(!flags.get_taken(), "Entry is already taken");
        assert!(!flags.get_present(), "Entry is already present");

        self.set_addr(phys_addr);
        self.set_flags(flags);
        self.set_flags(page_size.flag());
        self.set_flags(
            Flags::<X86_64>::new()
                .set_present(true)
                .set_last_entry(true),
        );
    }

    /// Marks the entry as not present and frees the physical page if the entry was activated not
    /// manually (ie. activated using a call to `activate`).
    fn release(&mut self) {
        let flags = self.get_flags();
        assert!(flags.get_present(), "Entry is not present");

        // If the entry was "taken" (ie. it was "activated" and not just mapped), we need to free
        // the physical page allocated to it
        if flags.get_taken() {
            let phys_addr = self.get_addr();
            unsafe {
                pmm::get().free(phys_addr, 1).expect("Failed to free page");
            }

            flags.set_taken(false);
        }
        flags.set_present(false);

        self.set_flags(flags);
    }
}

impl PageTable {
    /// Allocates a new page table
    pub fn new() -> (&'static mut Self, PhysAddr) {
        let phys_addr = pmm::get()
            .allocate(1, 1)
            .expect("Failed to allocate page table");

        // For easier bootstrapping, we are HHDM mapping all page tables
        let ptr: *mut u8 = phys_addr.add_hhdm_offset().into();
        // Memset to clear old stale data that might be in the page tables
        unsafe {
            memset(ptr, 0, size_of::<PageTable>());
        }

        (
            unsafe { ptr.cast::<PageTable>().as_mut().unwrap() },
            phys_addr,
        )
    }

    /// Tries to get a reference to the `Entry` associated with the given virtual address.
    ///
    /// If the entry is not present, `None` is returned.
    fn get_entry(&mut self, virt_addr: VirtAddr) -> Option<&mut Entry> {
        let mut table = self;

        for level in
            (PageSize::<X86_64>::size_4kb().bottom_paging_level()..MAX_BOTTOM_PAGING_LEVEL).rev()
        {
            let i = next_level_index(virt_addr, level);

            let flags = table[i].get_flags();
            // if table[i].is_flag_set(Entry::FLAG_LAST_ENTRY) {
            if flags.get_last_entry() {
                return Some(&mut table[i]);
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
    fn get_create_table_range(
        &mut self,
        base_addr: VirtAddr,
        page_size: PageSize<X86_64>,
    ) -> &mut PageTable {
        // TODO: Make sure address is aligned?
        // assert!((pages.end - pages.start) / BASIC_PAGE_SIZE <= ENTRIES_PER_TABLE, "Range out of bounds");

        let mut table = self;
        for level in (page_size.bottom_paging_level() + 1..=MAX_BOTTOM_PAGING_LEVEL).rev() {
            let i = next_level_index(base_addr, level);
            let flags = table[i].get_flags();
            if !flags.get_present() {
                table[i].set_addr(PageTable::new().1);
                flags.set_present(true).set_read_write(true);
                table[i].set_flags(flags);
            }

            table = table[i].next_level_table();
        }

        table
    }

    /// Tries to get the parent table of the given `base_addr`.
    ///
    /// If one of the page tables are missing during the translation, `None` is returned
    fn get_table_range(
        &mut self,
        base_addr: VirtAddr,
        page_size: PageSize<X86_64>,
    ) -> Option<&mut PageTable> {
        let mut table = self;
        for level in (page_size.bottom_paging_level() + 1..=MAX_BOTTOM_PAGING_LEVEL).rev() {
            let i = next_level_index(base_addr, level);
            let flags = table[i].get_flags();
            if !flags.get_present() {
                return None;
            }

            table = table[i].next_level_table();
        }

        Some(table)
    }

    /// Activates the mapping for the given virtual address
    ///
    /// If the entry isn't "taken" (or perhaps already activated) the function will panic
    fn activate_mapping(&mut self, base_addr: VirtAddr) {
        let entry = self.get_entry(base_addr).expect("Failed to get entry");

        entry.activate_taken();
    }

    /// Maps each of the virtual address composed of `virt_addr + i * page_size` so that when the page
    /// is activated a new physical addresses will be allocated and mapped to the entry
    pub fn map_allocate(
        &mut self,
        base_addr: VirtAddr,
        count: usize,
        page_size: PageSize<X86_64>,
        flags: Flags<X86_64>,
    ) {
        // TODO: Change me to the actual flags we can set and the ones we can't
        // assert!(flags & !0b111 == 0, "Invalid flags");

        // Get the parent page table
        let table = self.get_create_table_range(base_addr, page_size);

        // Find out how much to skip, and how much to take
        let to_skip = next_level_index(base_addr, page_size.bottom_paging_level());
        for entry in table.iter_mut().skip(to_skip).take(count) {
            entry.take(flags, page_size);
        }
    }

    /// Maps the given virtual address to the given physical address
    pub unsafe fn map(
        &mut self,
        base_addr: VirtAddr,
        phys_addr: PhysAddr,
        page_size: PageSize<X86_64>,
        flags: Flags<X86_64>,
    ) -> Result<(), PagingError> {
        // TODO: Change me to the actual flags we can set and the ones we can't
        // assert!(flags & !0b111 == 0, "Invalid flags");

        // Get the parent page table
        let table = self.get_create_table_range(base_addr, page_size);

        // Extract the index to the entry
        let i = next_level_index(base_addr, page_size.bottom_paging_level());
        // Map the entry
        unsafe { table[i].map(phys_addr, flags, page_size) };

        Ok(())
    }

    /// Unmaps the given virtual address range, as well as frees the physical page mapped to it if the page was
    /// mapped with `map_allocate`
    pub(super) unsafe fn unmap(
        &mut self,
        base_addr: VirtAddr,
        count: usize,
        page_size: PageSize<X86_64>,
    ) -> Result<(), PagingError> {
        let table = self.get_table_range(base_addr, page_size).unwrap();

        let to_skip = next_level_index(base_addr, page_size.bottom_paging_level());

        assert!(512 - to_skip >= count, "Freeing the requested page count starting this address will exceed this parent table and thus not possible.
            You should instead call the function individually for each parent page table");

        for entry in table.iter_mut().skip(to_skip).take(count) {
            entry.release();
        }

        Ok(())
    }

    /// Get the physical address associated with the given virtual address.
    ///
    /// If the virtual address is not mapped, `None` is returned.
    pub(super) fn translate(&mut self, base_addr: VirtAddr) -> Option<PhysAddr> {
        let entry = self.get_entry(base_addr)?;

        let flags = entry.get_flags();
        if !flags.get_present() {
            return None;
        }

        Some(entry.get_addr())
    }
}

/// Get the top level paging table PML4/PML5 (depending on the paging level)
pub(super) fn get_pml() -> &'static mut PageTable {
    let phys_addr = unsafe { PhysAddr((Cr3::read().top_pml() << 12) as usize) };

    let ptr: *mut PageTable = phys_addr.add_hhdm_offset().into();

    unsafe { ptr.cast::<PageTable>().as_mut().expect("Failed to get PML") }
}

#[inline]
const fn next_level_index(addr: VirtAddr, level: usize) -> usize {
    assert!(level < 5);

    (addr.0 >> (PageSize::size_4kb().offset_bit_count() + (level * 9))) & 0b1_1111_1111
}

#[cfg(feature = "limine")]
pub(super) unsafe fn init_from_limine(
    mem_map: &[&memory_map::Entry],
    kernel_virt: VirtAddr,
    kernel_phys: PhysAddr,
    used_by_pmm: &memory_map::Entry,
) {
    // TODO: CPUID check as well

    /// Helper function to avoid code duplication.
    ///
    /// Maps in the given virtual address to the given
    fn map_in_entry(
        base_virt_addr: VirtAddr,
        base_phys_addr: PhysAddr,
        page_count: usize,
        new_pml: &mut PageTable,
        flags: Flags<X86_64>,
    ) {
        // Just making sure the addresses are both aligned
        sanity_assert!(base_virt_addr.0 % X86_64::BASIC_PAGE_SIZE.size() == 0);
        sanity_assert!(base_phys_addr.0 % X86_64::BASIC_PAGE_SIZE.size() == 0);

        for i in 0..page_count {
            let virt_addr = base_virt_addr + (i * X86_64::BASIC_PAGE_SIZE.size());
            let phys_addr = base_phys_addr + (i * X86_64::BASIC_PAGE_SIZE.size());

            // TODO: Map with correct flags
            // TODO: Use different page sizes
            unsafe {
                new_pml
                    .map(virt_addr, phys_addr, PageSize::size_4kb(), flags)
                    .unwrap()
            };
        }
    }
    use logger::*;
    use pat::{PatEntry, PatType};

    use crate::BASIC_PAGE_SIZE;
    #[cfg(feature = "paging_5")]
    if read_cr!(cr4) & (1 << 12) != 0 {
        panic!("5 level paging requested, but not supported");
    }

    let (new_pml, new_pml_addr) = PageTable::new();

    map_in_entry(
        PhysAddr(used_by_pmm.length as usize).add_hhdm_offset(),
        PhysAddr(used_by_pmm.length as usize),
        used_by_pmm.length as usize / BASIC_PAGE_SIZE,
        new_pml,
        Flags::new().set_read_write(true),
    );

    // NOTE: We are doing the `EXECUTABLE_AND_MODULES` mapping independently of the other sections,
    // since the kernel's view of this section is different than HHDM (even though this memory is
    // also HHDM mapped IIRC)
    for entry in mem_map {
        // Just making sure the length is a multiple of a page size
        sanity_assert!(entry.length as usize % BASIC_PAGE_SIZE == 0);

        let page_count = entry.length as usize / BASIC_PAGE_SIZE;
        match entry.entry_type {
            EntryType::EXECUTABLE_AND_MODULES => map_in_entry(
                kernel_virt,
                kernel_phys,
                page_count,
                new_pml,
                Flags::new().set_read_write(true),
            ),
            EntryType::ACPI_RECLAIMABLE | EntryType::BOOTLOADER_RECLAIMABLE => map_in_entry(
                PhysAddr(entry.base as usize).add_hhdm_offset(),
                PhysAddr(entry.base as usize),
                page_count,
                new_pml,
                Flags::new().set_read_write(true),
            ),
            #[cfg(feature = "framebuffer")]
            EntryType::FRAMEBUFFER => map_in_entry(
                PhysAddr(entry.base as usize).add_hhdm_offset(),
                PhysAddr(entry.base as usize),
                page_count,
                new_pml,
                Flags::new()
                    .set_pat(PatType::WriteCombining, PageSize::size_4kb())
                    .set_read_write(true),
            ),
            _ => (),
        }
    }

    unsafe { finalize_init(new_pml_addr) };

    log_info!("Paging system initialized successfully");
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
            __cpuid(1).edx & NX_BIT != 0,
            "No-Execute (NX) bit is not supported by the CPU"
        );
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

/// Finalize the initialization of the paging system by moving over to the newly setup page table
unsafe fn finalize_init(pml_phys_addr: PhysAddr) {
    // TODO: Make sure CRs flags are OK
    unsafe {
        // Check to make sure the features we want to enable are supported
        setup_pat();
        check_pge_support();
        // enable_nx();

        let mut cr4 = Cr4::read();
        // Enable global pages support
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
