use core::arch::x86_64::__cpuid;
use core::fmt::Debug;
use core::num::NonZero;
use core::ops::{Deref, DerefMut};

use crate::arch::x86_64::cpu::{Cr3, Register};
use crate::arch::BASIC_PAGE_SIZE;
use crate::mem::{
    PhysAddr, VirtAddr,
    pmm::{self, PmmAllocator},
};
use limine::memory_map::{self, EntryType};
use pat::check_pat_support;
use utils::mem::memset;
use utils::sanity_assert;

use super::cpu::msr::{rdmsr, wrmsr, AmdMsr, Efer};
use super::cpu::Cr4;

mod pat;

/// The number of entries per page table
pub const ENTRIES_PER_TABLE: usize = 512;

/// The possible page sizes that can be used
#[derive(Debug, Clone, Copy)]
pub enum PageSize {
    /// 4KB page
    Size4KB = 0,
    /// 2MB page
    Size2MB = 1,
    /// 1GB page
    Size1GB = 2,
    #[cfg(feature = "paging_4")]
    Max = 3,
    #[cfg(feature = "paging_5")]
    Max = 4,
}

/// An entry in a page table
#[repr(C)]
#[derive(Debug)]
pub struct Entry(usize);

/// A page table
#[repr(C, align(4096))]
#[derive(Debug)]
pub struct PageTable([Entry; ENTRIES_PER_TABLE]);

#[allow(dead_code)]
impl Entry {
    /// Keep all flags as off
    pub const FLAGS_NONE: usize = 0;
    /// Present bit (`0`):
    /// - If `1` the page is present.
    /// - If `0` the page isn't present, and accessing it would cause a PF
    pub const FLAG_P: usize = 1 << 0;
    /// Read/write bit (`1`):
    /// - If `1` any address this page maps is writeable and readable
    /// - If `0` any address this page maps is read-only
    pub const FLAG_RW: usize = 1 << 1;
    /// User/supervisor bit (`2`):
    /// - If `1` any address this page maps is accessible from user mode
    /// - If `0` any address this page maps is accessible only from kernel mode
    pub const FLAG_US: usize = 1 << 2;
    /// Page-level write-through bit (`3`):
    /// - If `1` the page is write-through
    /// - If `0` the page is write-back
    pub const FLAG_PWT: usize = 1 << 3;
    /// Page-level cache disable bit (`4`):
    /// - If `1` the page is not cacheable
    /// - If `0` the page is cacheable
    pub const FLAG_PCD: usize = 1 << 4;
    /// Accessed bit (`5`):
    /// - If `1` the page has been accessed (read from or written to)
    /// - If `0` the page has not been accessed
    pub const FLAG_A: usize = 1 << 5;
    /// Dirty bit (`6`):
    /// - If `1` the page has been written to
    /// - If `0` the page has not been written to
    pub const FLAG_D: usize = 1 << 6;
    /// Page size bit (`7`):
    /// - If `1` the page is 2MB
    /// - If `0` the page is 4KB
    pub const FLAG_PS: usize = 1 << 7;
    /// Global bit (`8`):
    /// - If `1` the CPU won't update the associated address when the TLB is flushed
    /// - If `0` the CPU will update the associated address when the TLB is flushed
    pub const FLAG_G: usize = 1 << 8;
    /// Ignored bits (`9-11`) on AMD:
    /// Ignored bits (`9-10`) on Intel:
    pub const RESERVED: usize = 1 << 9;
    /// HLAT paging bit (`12` Intel only):
    #[cfg(feature = "intel")]
    pub const HLAT: usize = 1 << 11;
    /// PAT bit (`12`) (on PDPE!)
    pub const FLAG_1GB_PAT: usize = 1 << 12;
    /// PAT bit (`7`) (on PTE!)
    pub const FLAG_4KB_PAT: usize = 1 << 7;
    /// Execute disable bit (`63`):
    /// - If `1` the page is not executable
    /// - If `0` the page is executable
    pub const FLAG_XD: usize = 1 << 63;

    /// A custom flag to mark the entry as taken - meaning that the entry is no free, but isn't
    /// present (ie. not useable yet).
    /// It will become present when the entry is activated, so we can lazily map the page.
    pub const FLAG_TAKEN: usize = 1 << 9;

    /// When lazily mapping a page, we can't pass information regarding the size of the page we
    /// want to map.
    /// So we need to set this flag to mark the entry as a "last entry" in the page table, and now
    /// that we know the last level we can combine that with the `PS` flag to determine the size
    pub const FLAG_LAST_ENTRY: usize = 1 << 10;

    /// Set a flag on
    const fn set_flag(&mut self, flag: usize) {
        self.0 |= flag;
    }

    /// Set a flag off
    const fn clear_flag(&mut self, flag: usize) {
        self.0 &= !flag;
    }

    /// Returns `true` if the given flag is set, `false` otherwise
    const fn is_flag_set(&self, flag: usize) -> bool {
        (self.0 & flag) != 0
    }

    /// Sets the entry's address to the given physical address
    const fn set_addr(&mut self, addr: PhysAddr) {
        // Mask the address to make sure it's aligned
        assert!(addr.0.trailing_zeros() >= 12, "Address is not aligned");

        self.0 |= addr.0;
    }

    /// Returns the entry's physical address
    const fn get_addr(&self) -> PhysAddr {
        // Possibly mask this address
        PhysAddr(self.0 & !0xFFF)
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
    fn take(&mut self, flags: usize, page_size: PageSize) {
        assert!(
            !self.is_flag_set(Self::FLAG_TAKEN),
            "Entry is already taken"
        );
        assert!(!self.is_flag_set(Self::FLAG_P), "Entry is already present");

        self.set_flag(flags);
        self.set_flag(page_size.flag());
        self.set_flag(Self::FLAG_TAKEN);
        self.set_flag(Self::FLAG_LAST_ENTRY);

        let phys_addr = pmm::get()
            .allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap())
            .expect("Failed to allocate page");

        self.set_addr(phys_addr);
        self.set_flag(Self::FLAG_P);
        // XXX: Maybe need to memset to 0?
        // set the taken bit
        // set the other requested flags
    }

    /// Activates a "taken" entry.
    ///
    /// Most setting up was already done by `take()`, all we need to do now is allocate a physical
    /// page and map the virtual address to it, as well as set the `present` bit.
    fn activate_taken(&mut self) {
        assert!(self.is_flag_set(Self::FLAG_TAKEN), "Entry is not taken");
        assert!(!self.is_flag_set(Self::FLAG_P), "Entry is already present");

        let phys_addr = pmm::get()
            .allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap())
            .expect("Failed to allocate page");

        self.set_addr(phys_addr);
        self.set_flag(Self::FLAG_P);
    }

    /// Immediately maps the entry to the given physical address with the given flags.
    unsafe fn map(&mut self, phys_addr: PhysAddr, flags: usize, page_size: PageSize) {
        assert!(!self.is_flag_set(Self::FLAG_P), "Entry is not present");
        assert!(
            !self.is_flag_set(Self::FLAG_TAKEN),
            "Entry is already taken"
        );

        self.set_addr(phys_addr);
        self.set_flag(flags);
        self.set_flag(page_size.flag());
        self.set_flag(Self::FLAG_P);
        self.set_flag(Self::FLAG_LAST_ENTRY);
    }

    /// Marks the entry as not present and frees the physical page if the entry was activated not
    /// manually (ie. activated using a call to `activate`).
    fn release(&mut self) {
        assert!(self.is_flag_set(Self::FLAG_P), "Entry is not present");

        // If the entry was "taken" (ie. it was "activated" and not just mapped), we need to free
        // the physical page allocated to it
        if self.is_flag_set(Self::FLAG_TAKEN) {
            let phys_addr = self.get_addr();
            unsafe {
                pmm::get()
                    .free(phys_addr, NonZero::new(1).unwrap())
                    .expect("Failed to free page");
            }

            self.clear_flag(Self::FLAG_TAKEN);
        }

        self.clear_flag(Self::FLAG_P);
    }
}

impl PageTable {
    /// Allocates a new page table
    pub fn new() -> (&'static mut Self, PhysAddr) {
        let phys_addr = pmm::get()
            .allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap())
            .expect("Failed to allocate page table");

        // For easier bootstrapping, we are HHDM mapping all page tables
        let ptr: *mut u8 = phys_addr.add_hhdm_offset().into();
        // Memset to clear old stale data that might be in the page tables
        unsafe {
            memset(ptr, 0, core::mem::size_of::<PageTable>());
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
            (PageSize::Size4KB.bottom_paging_level()..=PageSize::Max.bottom_paging_level()).rev()
        {
            let i = virt_addr.next_level_index(level);

            if table[i].is_flag_set(Entry::FLAG_LAST_ENTRY) {
                return Some(&mut table[i]);
            } else if table[i].is_flag_set(Entry::FLAG_P) {
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
        page_size: PageSize,
    ) -> &mut PageTable {
        {
            let start = base_addr.next_level_index(page_size.bottom_paging_level());
            let end = start + (page_size as usize);
            assert!(end <= ENTRIES_PER_TABLE, "Range out of bounds");
        }

        // TODO: Make sure address is aligned?
        // assert!((pages.end - pages.start) / BASIC_PAGE_SIZE <= ENTRIES_PER_TABLE, "Range out of bounds");

        let mut table = self;
        for level in
            (page_size.bottom_paging_level() + 1..=PageSize::Max.bottom_paging_level()).rev()
        {
            let i = base_addr.next_level_index(level);
            if !table[i].is_flag_set(Entry::FLAG_P) {
                table[i].set_addr(PageTable::new().1);
                table[i].set_flag(Entry::FLAG_P);
                table[i].set_flag(Entry::FLAG_RW);
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
        page_size: PageSize,
    ) -> Option<&mut PageTable> {
        {
            let start = base_addr.next_level_index(page_size.bottom_paging_level());
            let end = start + (page_size as usize);
            assert!(end <= ENTRIES_PER_TABLE, "Range out of bounds");
        }

        let mut table = self;
        for level in
            (page_size.bottom_paging_level() + 1..=PageSize::Max.bottom_paging_level()).rev()
        {
            let i = base_addr.next_level_index(level);
            if !table[i].is_flag_set(Entry::FLAG_P) {
                return None;
            }

            table = table[i].next_level_table();
        }

        Some(table)
    }

    /// Activates the mapping for the given virtual address
    ///
    /// If the entry isn't "taken" (or perhaps already activated) the function will panic
    pub fn activate_mapping(&mut self, base_addr: VirtAddr) {
        let entry = self.get_entry(base_addr).expect("Failed to get entry");

        entry.activate_taken();
    }

    /// Maps each of the virtual address composed of `virt_addr + i * page_size` so that when the page
    /// is activated a new physical addresses will be allocated and mapped to the entry
    pub fn map_allocate(
        &mut self,
        base_addr: VirtAddr,
        count: usize,
        page_size: PageSize,
        flags: usize,
    ) {
        // TODO: Change me to the actual flags we can set and the ones we can't
        assert!(flags & !0b111 == 0, "Invalid flags");

        // Get the parent page table
        let table = self.get_create_table_range(base_addr, page_size);

        // Find out how much to skiop how much to take
        let to_skip = base_addr.next_level_index(page_size.bottom_paging_level());
        for entry in table.iter_mut().skip(to_skip).take(count) {
            entry.take(flags, page_size);
        }
    }

    /// Maps the given virtual address to the given physical address
    pub unsafe fn map(
        &mut self,
        base_addr: VirtAddr,
        phys_addr: PhysAddr,
        page_size: PageSize,
        flags: usize,
    ) {
        // TODO: Change me to the actual flags we can set and the ones we can't
        assert!(flags & !0b111 == 0, "Invalid flags");

        // Get the parent page table
        let table = self.get_create_table_range(base_addr, page_size);

        // Extract the index to the entry
        let i = base_addr.next_level_index(page_size.bottom_paging_level());
        // Map the entry
        unsafe { table[i].map(phys_addr, flags, page_size) };
    }

    /// Unmaps the given virtual address range, as well as frees the physical page mapped to it if the page was
    /// mapped with `map_allocate`
    pub unsafe fn unmap(&mut self, base_addr: VirtAddr, count: usize, page_size: PageSize) {
        let table = self.get_table_range(base_addr, page_size).unwrap();

        let to_skip = base_addr.next_level_index(page_size.bottom_paging_level());

        assert!(512 - to_skip >= count, "Freeing the requested page count starting this address will exceed this parent table and thus not possible.
            You should instead call the function individually for each parent page table");

        for entry in table.iter_mut().skip(to_skip).take(count) {
            entry.release();
        }
    }

    /// Get the physical address associated with the given virtual address.
    ///
    /// If the virtual address is not mapped, `None` is returned.
    pub fn translate(&mut self, base_addr: VirtAddr) -> Option<PhysAddr> {
        let entry = self.get_entry(base_addr)?;

        if entry.is_flag_set(Entry::FLAG_P) {
            Some(entry.get_addr())
        } else {
            None
        }
    }
}

impl PageSize {
    /// Get the bottom paging level of the page size (ie. the level of the page table that maps
    /// entries of this size)
    #[inline]
    pub const fn bottom_paging_level(self) -> usize {
        self as usize
    }

    /// Get the flag that needs to be set to make the entry map a page of this size
    #[inline]
    const fn flag(self) -> usize {
        match self {
            PageSize::Size4KB => 0,
            _ => Entry::FLAG_PS,
        }
    }

    /// Get the amount of bits of the offset in the virtual address
    #[inline]
    pub const fn offset_bit_count(self) -> usize {
        12 + (self as usize * 9)
    }

    // TODO: Possibly have the enum contian this value instead of calculating it every time if the
    // usage is high enough
    /// Get the size in bytes of this page size
    #[inline]
    pub const fn size(self) -> usize {
        2_usize.pow(self.offset_bit_count() as u32)
    }
}

/// Get the top level paging table PML4/PML5 (depending on the paging level)
pub fn get_pml() -> &'static mut PageTable {
    let phys_addr = unsafe {
        PhysAddr((Cr3::read().top_pml() << 12) as usize)
    };

    let ptr: *mut PageTable = phys_addr.add_hhdm_offset().into();

    unsafe { ptr.cast::<PageTable>().as_mut().expect("Failed to get PML") }
}

/// Helper function to avoid code duplication.
///
/// Maps in the given virtual address to the given
fn map_with_hhdm_offset(
    base_virt_addr: VirtAddr,
    base_phys_addr: PhysAddr,
    page_count: usize,
    new_pml: &mut PageTable,
) {
    // Just making sure the addresses are both aligned
    sanity_assert!(base_virt_addr.0 % BASIC_PAGE_SIZE == 0);
    sanity_assert!(base_phys_addr.0 % BASIC_PAGE_SIZE == 0);

    for i in 0..page_count {
        let virt_addr = base_virt_addr + (i * BASIC_PAGE_SIZE);
        let phys_addr = base_phys_addr + (i * BASIC_PAGE_SIZE);

        unsafe { new_pml.map(virt_addr, phys_addr, PageSize::Size4KB, Entry::FLAG_RW) };
    }
}

/// Initialize the paging subsystem when booting from Limine
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine(
    mem_map: &[&memory_map::Entry],
    kernel_virt: VirtAddr,
    kernel_phys: PhysAddr,
) {
    // TODO: CPUID check as well
    #[cfg(feature = "paging_5")]
    if read_cr!(cr4) & (1 << 12) != 0 {
        panic!("5 level paging requested, but not supported");
    }

    let (new_pml, new_pml_addr) = PageTable::new();

    // TODO: Map only some portions of the USEABLE memory that is for HHDM mapped stuff
    // Mapping in all of the memory we need.
    //
    // NOTE: We are doing the `EXECUTABLE_AND_MODULES` mapping independently of the other sections,
    // since the kernel's view of this section is different than HHDM (even though this memory is
    // also HHDM mapped IIRC)
    for entry in mem_map {
        // Just making sure the length is a multiple of a page size
        sanity_assert!(entry.length as usize % BASIC_PAGE_SIZE == 0);

        let page_count = entry.length as usize / BASIC_PAGE_SIZE;
        match entry.entry_type {
            EntryType::EXECUTABLE_AND_MODULES => {
                map_with_hhdm_offset(kernel_virt, kernel_phys, page_count, new_pml)
            }
            EntryType::ACPI_RECLAIMABLE | EntryType::BOOTLOADER_RECLAIMABLE => {
                map_with_hhdm_offset(
                    PhysAddr(entry.base as usize).add_hhdm_offset(),
                    PhysAddr(entry.base as usize),
                    page_count,
                    new_pml,
                )
            }
            EntryType::USABLE => map_with_hhdm_offset(
                PhysAddr(entry.base as usize).add_hhdm_offset(),
                PhysAddr(entry.base as usize),
                page_count,
                new_pml,
            ),
            #[cfg(feature = "framebuffer")]
            EntryType::FRAMEBUFFER => map_with_hhdm_offset(
                PhysAddr(entry.base as usize).add_hhdm_offset(),
                PhysAddr(entry.base as usize),
                page_count,
                new_pml,
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
        if __cpuid(1).edx & PGE_BIT == 0 {
            panic!("Paging Global Enable (PGE) is not supported by the CPU");
        }
    }
}

/// Check if the CPU supports NX (No-Execute) bit.
#[inline]
fn check_nx_support() {
    const NX_BIT: u32 = 1 << 20;

    unsafe {
        if __cpuid(1).edx & NX_BIT == 0 {
            panic!("No-Execute (NX) bit is not supported by the CPU");
        }
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
        check_pat_support();
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
