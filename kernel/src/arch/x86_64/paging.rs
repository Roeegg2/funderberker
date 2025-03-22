//! The core x86_64 paging mechanism

#[cfg(all(feature = "paging_4", feature = "paging_5"))]
compiler_error!("Can't have both 4 level and 5 level paging. Choose one of the options");
#[cfg(not(any(feature = "paging_4", feature = "paging_5")))]
compiler_error!("No paging level is selected. Choose one of the options");

#[cfg(feature = "limine")]
use limine::memory_map;

use crate::{
    mem::{
        PhysAddr, VirtAddr,
        pmm::{PmmAllocator, PmmError},
    },
    read_cr, write_cr,
};

#[cfg(feature = "paging_4")]
const PAGING_LEVEL: u8 = 4;
#[cfg(feature = "paging_5")]
const PAGING_LEVEL: u8 = 5;

const ENTRIES_PER_TABLE: usize = 512;

pub const BASIC_PAGE_SIZE: usize = 0x1000;

/// Errors that the paging system might encounter
#[derive(Debug, Copy, Clone)]
pub enum PagingError {
    /// The address is not aligned to the page size
    UnalignedAddress,
    /// All entries are marked as full even though the PT is said to have free slots (AVL bits aren't set)
    UnexpectedlyTableFull(u8),
    /// PMM page allocator failure whilst trying to allocate a paging table
    AllocationError(PmmError),
    /// Conversion of a PageTable's `Entry` to a `PageTable` failed
    EntryToPageTableFailed,
    /// Virtual address asked to be mapped is already reserved
    AlreadyReservedVirtualAddress(u8),
    /// Virtual address asked to be unmapped is already free
    AlreadyFreeVirtualAddress(u8),
    /// One (or possibly more) paging tables were found missing during virtual address translation
    /// at level _
    MissingPagingTable(u8),
}

#[derive(Debug, Clone, Copy)]
pub enum PageSize {
    Size4KB = 0,
    Size2MB = 1,
    Size1GB = 2,
}

impl PageSize {
    #[inline]
    const fn get_size(self) -> usize {
        BASIC_PAGE_SIZE * (ENTRIES_PER_TABLE.pow(self as u32))
    }

    #[inline]
    const fn get_paging_level(self) -> u8 {
        PAGING_LEVEL - 1 - (self as u8)
    }

    #[inline]
    const fn get_flag(self) -> usize {
        match self {
            PageSize::Size4KB => 0,
            _ => Entry::FLAG_PS,
        }
    }

    #[inline]
    const fn get_shift(self) -> usize {
        12 + (self as usize * 9)
    }
}

/// A paging table entry
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct Entry(usize);

#[allow(dead_code)]
impl Entry {
    /// Present bit - 0 => not present. 1 => present
    const FLAG_P: usize = 1 << 0;
    /// Read/write - 0 => just read. 1 => read + write
    const FLAG_RW: usize = 1 << 1;
    /// User/supervisor - 0 => only CPL0,1,2. 1 => CPL3 as well
    const FLAG_US: usize = 1 << 2;
    /// Page-level writethrough - 0 => writeback. 1 => writethough caching.
    const FLAG_PWT: usize = 1 << 3;
    /// Page-level cache disable - 0 => cacheable. 1 => non cacheable.
    const FLAG_PCD: usize = 1 << 4;
    /// Accessed - 0 => not accessed yet. 1 => page was read/writted to.
    const FLAG_A: usize = 1 << 5;
    const _FLAG_IGN: usize = 1 << 6;
    /// (on PTE only!) dirty - 0 => page not written to. 1 => page was written to.
    const FLAG_D: usize = 1 << 6;
    /// (on PDE only!) page size - 0 => page is 4KB. 1 => page is 2MB. should be set to 0 for all other tables
    const FLAG_PS: usize = 1 << 7;
    const FLAG_PAT: usize = 1 << 7;
    const _FLAG_MBZ: usize = 0b11 << 7; // (on PML4E/PML5E only!)
    const _FLAG_IGN_2: usize = 1 << 8; // on PDE/PDPE only!
    const FLAG_G: usize = 1 << 8; // on PTE only!
    /// All possible flags turned on
    const FLAG_ALL: usize = 0b0111_1111_1111;

    const FLAG_AVL: usize = 0b111 << 9;
}

impl Entry {
    /// Set address or flags to entry
    const fn set(&mut self, data: usize) {
        self.0 |= data;
    }

    /// Unset the flags from the entry
    const fn unset_flags(&mut self, flags: usize) {
        self.0 &= !flags
    }

    /// Get the flags from the entry
    const fn get_flags(&self, flag: usize) -> usize {
        self.0 & flag
    }
}

impl From<Entry> for PhysAddr {
    fn from(mut value: Entry) -> Self {
        value.unset_flags(Entry::FLAG_ALL);
        Self(value.0)
    }
}

impl<'a> TryFrom<Entry> for &'a mut PageTable {
    type Error = PagingError;

    // NOTE: Have this marked as `unsafe`? Could result in UB if address inside `value` is invalid
    /// Convert Entry into an address, and using that address into
    fn try_from(value: Entry) -> Result<Self, Self::Error> {
        unsafe {
            // Extract the phys addr outside of `value`, add HHDM offset so it's a valid kernel virtual address
            let ptr = core::ptr::without_provenance_mut::<PageTable>(
                PhysAddr::from(value).add_hhdm_offset().0,
            );
            ptr.as_mut().ok_or(PagingError::EntryToPageTableFailed)
        }
    }
}

unsafe fn map_page_range(
    pml: &mut PageTable,
    virt_addr_start: VirtAddr,
    phys_addr_start: PhysAddr,
    len: usize,
    flags: usize,
) -> Result<(), PagingError> {
    for i in (0..len).step_by(BASIC_PAGE_SIZE) {
        let pte = {
            // Shift it 12 bits to the left, since it's page aligned address
            let virt_addr = VirtAddr((virt_addr_start.0 + i) >> 12);
            pml.get_create_entry_specific(virt_addr, PAGING_LEVEL - 1)
        }?;

        // Populate the PTE with the desired PhysAddr + Flags
        pte.set((phys_addr_start.0 + i) | flags);
    }

    Ok(())
}

/// Finalize the paging system initialization
#[inline]
unsafe fn final_init(pml_addr: PhysAddr) {
    // TODO: Make sure CRs flags are OK (if we're not booting with Limine)
    write_cr!(cr3, pml_addr.0);

    //write_cr!(cr4, )
}

/// Initialize the paging system from the limine
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine(
    mem_map: &[&memory_map::Entry],
    kernel_virt: VirtAddr,
    kernel_phys: PhysAddr,
) -> Result<(), PagingError> {
    let (pml, pml_addr) = PageTable::new()?;

    #[cfg(feature = "paging_5")]
    (read_cr!(cr4) | (1 << 12) != 0)
        .then(|| ())
        .expect("5 level paging requested, but not supported");

    for entry in mem_map {
        match entry.entry_type {
            // Although the pages in this entry are also accessible from HHDM, they are mapped to a
            // different region of virtual memory the kernel, and the kernel executes from there,
            // so mapping it with HHDM as well won't work.
            memory_map::EntryType::KERNEL_AND_MODULES => unsafe {
                map_page_range(
                    pml,
                    kernel_virt,
                    kernel_phys,
                    entry.length as usize,
                    Entry::FLAG_RW | Entry::FLAG_P,
                )
            }?,

            // TODO: Instead of just mapping THE WHOLE physical address as HHDM, just map the page
            // tables + ACPI tables. When the kernel will need more memory, it'll ask for it to be
            // mapped regurarly with HHDM offset
            memory_map::EntryType::ACPI_RECLAIMABLE
            | memory_map::EntryType::BOOTLOADER_RECLAIMABLE
            | memory_map::EntryType::USABLE => unsafe {
                let phys_addr = PhysAddr(entry.base as usize);
                map_page_range(
                    pml,
                    phys_addr.add_hhdm_offset(),
                    phys_addr,
                    entry.length as usize,
                    Entry::FLAG_RW | Entry::FLAG_P,
                )
            }?,
            #[cfg(feature = "framebuffer")]
            memory_map::EntryType::FRAMEBUFFER => unsafe {
                let phys_addr = PhysAddr(entry.base as usize);
                map_page_range(
                    pml,
                    phys_addr.add_hhdm_offset(),
                    phys_addr,
                    entry.length as usize,
                    Entry::FLAG_RW | Entry::FLAG_P,
                )
            }?,

            // We don't care about the rest of the entries
            _ => (),
        }
    }

    unsafe { final_init(pml_addr) };

    log!("Setup paging successfully!");

    Ok(())
}

/// A paging table in the paging tree
#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable([Entry; ENTRIES_PER_TABLE]);

impl PageTable {
    // NOTE: Not sure whether this should be static or not...
    /// Get the PML4/PML5 (depending on whether 4 or 5 level paging is enabled) from CR3
    #[inline]
    unsafe fn get_pml() -> Result<&'static mut PageTable, PagingError> {
        let addr = Entry(read_cr!(cr3));
        addr.try_into()
    }

    /// Tries to map the given physical address to some available virtual address
    pub fn map_page_any(
        phys_addr: PhysAddr,
        flags: usize,
        page_size: PageSize,
    ) -> Result<VirtAddr, PagingError> {
        //let page_size = page_size.get_size();
        let pml = unsafe { PageTable::get_pml() }?;
        let (pte, virt_addr) = pml.get_create_entry_any(page_size.get_paging_level())?;
        pte.0 = phys_addr.0 | flags | page_size.get_flag();

        Ok(virt_addr)
    }

    /// Tries to map the given physical address to the given virtual address.
    pub fn map_page_specific(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        flags: usize,
        page_size: PageSize,
    ) -> Result<(), PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let pte = pml.get_create_entry_specific(
            VirtAddr(virt_addr.0 >> page_size.get_shift()),
            page_size.get_paging_level(),
        )?;
        pte.0 = phys_addr.0 | flags | page_size.get_flag();

        Ok(())
    }

    pub unsafe fn unmap_page(virt_addr: VirtAddr, page_size: PageSize) -> Result<(), PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let pte = pml.get_entry_specific(
            VirtAddr(virt_addr.0 >> page_size.get_shift()),
            page_size.get_paging_level(),
        )?;

        pte.unset_flags(Entry::FLAG_P);

        Ok(())
    }

    // TODO: Add a mechanism to keep track of whether we should free the paging structures or
    // rather keep them in memory for a near allocation
    /// Marks the the PTE associated with given virtual address as not present and LRU age 0
    /// NOTE: Make sure the entry isn't used before doing any modifications to it!
    fn get_entry_specific(
        &mut self,
        virt_addr: VirtAddr,
        level: u8,
    ) -> Result<&mut Entry, PagingError> {
        // NOTE: Because of the bitwise and, next index can't be more than 511 so it's safe to
        // index using it without checks
        let index = (virt_addr.0 >> (level * 9)) & 0b1_1111_1111;
        if level == 0 {
            return Ok(&mut self.0[index]);
        }

        // Can't get an entry which one of the tables in it's path is unmapped...
        if self.0[index].get_flags(Entry::FLAG_P) == 0 {
            return Err(PagingError::MissingPagingTable(level));
        }

        let next_table: &mut PageTable = self.0[index].try_into()?;
        next_table.get_entry_specific(virt_addr, level - 1)
    }

    /// Tries to get the PTE of a virtual address. Will allocate any tables if it needs to
    /// make given virtual address addressable
    /// NOTE: Make sure the entry isn't used before doing any modifications to it!
    fn get_create_entry_specific(
        &mut self,
        virt_addr: VirtAddr,
        level: u8,
    ) -> Result<&mut Entry, PagingError> {
        // NOTE: Because of the bitwise and, next index can't be more than 511 so it's safe to
        // index using it without checks
        let index = (virt_addr.0 >> (level * 9)) & 0b1_1111_1111;
        if level == 0 {
            return Ok(&mut self.0[index]);
        }

        // If the current entry is associated with a present entry, use it. Otherwise allocate
        // a new table
        let next_table = {
            if self.0[index].get_flags(Entry::FLAG_P) != 0 {
                self.0[index].try_into()?
            } else {
                let (next_table, phys_addr) = PageTable::new()?;
                self.0[index].set(phys_addr.0 | Entry::FLAG_P | Entry::FLAG_RW);
                next_table
            }
        };

        next_table.get_create_entry_specific(virt_addr, level - 1)
    }

    /// Tries to get any free PTE. If there isn't one available, it tries to creates one.
    /// NOTE: Make sure the entry isn't used before doing any modifications to it!
    fn get_create_entry_any(&mut self, level: u8) -> Result<(&mut Entry, VirtAddr), PagingError> {
        // If we reached PTE
        if level == 0 {
            // Find the first non present (ie not taken) entry
            let (index, entry) = self
                .0
                .iter_mut()
                .enumerate()
                .find(|pte| pte.1.get_flags(Entry::FLAG_P) == 0)
                .ok_or(PagingError::UnexpectedlyTableFull(0))?;

            // Return mutable ref to entry, and the index used
            return Ok((entry, VirtAddr(index)));
        }

        for entry in self.0.iter_mut().enumerate() {
            // If the current entry is associated with a present entry, use it. Otherwise allocate
            // a new table
            let next_table = {
                if entry.1.get_flags(Entry::FLAG_P) != 0 {
                    (*entry.1).try_into()?
                } else {
                    let (next_table, phys_addr) = PageTable::new()?;
                    entry.1.set(phys_addr.0 | Entry::FLAG_P | Entry::FLAG_RW);
                    next_table
                }
            };

            // If at the previous level we encountered an `UnexpectedlyTableFull`, try calling the
            // next. Otherwise, just return whatever the functions below returned
            match next_table.get_create_entry_any(level - 1) {
                Err(PagingError::UnexpectedlyTableFull(_)) if entry.0 < ENTRIES_PER_TABLE - 1 => {
                    continue;
                }
                any => return any,
            };
        }

        // If all further calls return `UnexpectedlyTableFull`, that means all of this tables
        // entries are `UnexpectedlyTableFull` and, so return that error
        Err(PagingError::UnexpectedlyTableFull(level))
    }

    // NOTE: Again, note sure if `static` is the correct lifetime here
    // Make the current table entry point to a new PageTable, and return that PageTable
    fn new() -> Result<(&'static mut PageTable, PhysAddr), PagingError> {
        // Get the physical address reserved for the table (it's exactly 1 table, 1 page alignment)
        let phys_addr = crate::mem::pmm::get()
            .alloc_any(1, 1)
            .map_err(|e| PagingError::AllocationError(e))?;

        let page_table = unsafe {
            // HHDM convert PhysAddr -> VirtAddr and then to a viable pointer
            let ptr = core::ptr::without_provenance_mut(phys_addr.add_hhdm_offset().0);
            // Important! Memset to get rid of old data
            utils::mem::memset(ptr, 0, BASIC_PAGE_SIZE);

            // TODO: Change this error to something more meaningfull
            (ptr as *mut PageTable)
                .as_mut()
                .ok_or(PagingError::EntryToPageTableFailed)
        }?;

        Ok((page_table, phys_addr))
    }
}

// PCIDE & TLB
// 5 level paging
// different page sizes
// slab allocator
// page fault handler
