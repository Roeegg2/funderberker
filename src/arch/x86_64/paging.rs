//! The core x86_64 paging mechanism

#[cfg(all(feature = "paging_4", feature = "paging_5"))]
compiler_error!("Can't have both 4 level and 5 level paging. Choose one of the options");
#[cfg(not(any(feature = "paging_4", feature = "paging_5")))]
compiler_error!("No paging level is selected. Choose one of the options");

use crate::{
    mem::{PhysAddr, VirtAddr, pmm::PmmError},
    read_cr, write_cr,
};

#[cfg(feature = "paging_4")]
const PAGING_LEVEL: u8 = 4;
#[cfg(feature = "paging_5")]
const PAGING_LEVEL: usize = 5;

const ENTRIES_PER_TABLE: usize = 512;

#[derive(Debug)]
pub enum PagingError {
    /// All entries are marked as full even though the PT is said to have free slots (AVL bits aren't set)
    UnexpectedlyTableFull(u8),
    /// PMM page allocator failure whilst trying to allocate a paging table
    AllocationError(PmmError),
    /// Conversion of a PageTable's `Entry` to a `PageTable` failed
    EntryToPageTableFailed,
    /// Virtual address asked to be mapped is already reserved
    AlreadyReservedVirtualAddress(u8),
    /// VIrtual address asked to be unmapped is already free
    AlreadyFreeVirtualAddress(u8),
    /// One (or possibly more) paging tables were found missing during virtual address translation
    /// at level _
    MissingPagingTable(u8),
}

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

    /// The following flags are for our use, so I've given them my own meaning:
    const FLAG_AVL: usize = 0b111 << 9;
    /// 0 means unreserved, free for use by whoever finds this table.
    /// NOTE: It thus only makes sense to have `FLAG_AVL == 0` if `FLAG_P == 0` as well. But we can
    /// definitely have `FLAG_AVL != 0` and `FLAG_P == 0`.
    /// `FLAG_P` simple means I can access the virtual address/es addressed by this entry. I can
    /// have an entry be reserved, but not yet accessible.
    /// `free` is the opposite of `taken`, not `accessible`.
    const FLAG_RESERVED: usize = 0b1 << 9;
    /// Some flags I'll use for LRU, not relevant now though
    const _FLAG_LRU: usize = 0b11 << 10;
}

impl Entry {
    // Set address or flags to entry
    const fn set(&mut self, data: usize) {
        self.0 |= data;
    }

    const fn unset_flags(&mut self, flags: usize) {
        self.0 &= !flags
    }

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

    // TODO: Have this marked as `unsafe`? Could result in UB if address inside `value` is invalid
    /// Convert Entry into an address, and using that address into
    fn try_from(value: Entry) -> Result<Self, Self::Error> {
        unsafe {
            // Extract the phys addr outside of `value`, add HHDM offset so it's a valid VMM virtual address
            let ptr = core::ptr::without_provenance_mut::<PageTable>(
                PhysAddr::from(value).add_hhdm_offset().0,
            );
            ptr.as_mut().ok_or(PagingError::EntryToPageTableFailed)
        }
    }
}

/// Set up paging using Limine's memory map
/// NOTE: SHOULD ONLY BE CALLED ONCE DURING BOOT!
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) -> Result<(), PagingError> {
    let (pml, pml_addr) = PageTable::new()?;

    for entry in mem_map {
        match entry.entry_type {
            // TODO: Framebuffer only if #[cfg(feature = "framebuffer")]
            limine::memory_map::EntryType::ACPI_RECLAIMABLE
            | limine::memory_map::EntryType::KERNEL_AND_MODULES
            | limine::memory_map::EntryType::FRAMEBUFFER => {
                let page_range = {
                    let start_phys_page = entry.base as usize;
                    let end_phys_page = start_phys_page + (entry.length as usize);

                    (start_phys_page..end_phys_page).step_by(0x1000)
                };

                // Map each physical page in the range to it's HHDM virtual page
                page_range.into_iter().try_for_each(|phys_page| {
                    let phys_page = PhysAddr(phys_page);
                    let virt_page = phys_page.add_hhdm_offset();
                    let pte = pml
                        .get_create_entry_specific(VirtAddr(virt_page.0 >> 12), PAGING_LEVEL - 1)?;
                    // Set to actually correct permissions
                    pte.set(phys_page.0 | Entry::FLAG_P | Entry::FLAG_RW);

                    Ok(())
                })?;
            }
            _ => (),
        }
    }

    log!("Filled up page tables successfully!");

    write_cr!(cr3, pml_addr.0);

    log!("Loaded CR3 successfully!");

    Ok(())
}

#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable([Entry; 512]);

// LATETODO: Try to TCE optimize each of the recursive functions
// LATETODO: Maybe implement `unmap_tree` and `allocate_tree` functions?
// TODO: Add support for PCIDE
// TODO: Add support for multiple page sizes
impl PageTable {
    // NOTE: Not sure whether this should be static or not...
    /// Get the PML4/PML5 (depending on whether 4 or 5 level paging is enabled) from CR3
    #[inline]
    unsafe fn get_pml() -> Result<&'static mut PageTable, PagingError> {
        let addr = Entry(read_cr!(cr3));
        addr.try_into()
    }

    /// Tries to map the given physical address to some available virtual address
    pub fn map_page_any(phys_addr: PhysAddr, flags: usize) -> Result<VirtAddr, PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let (pte, virt_addr) = pml.get_create_entry_any(PAGING_LEVEL - 1)?;
        pte.set(phys_addr.0 | flags);

        Ok(virt_addr)
    }

    /// Tries to map the given physical address to the given virtual address.
    pub fn map_page_specific(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        flags: usize,
    ) -> Result<(), PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let pte = pml.get_create_entry_specific(VirtAddr(virt_addr.0 >> 12), PAGING_LEVEL - 1)?;
        pte.set(phys_addr.0 | flags);

        Ok(())
    }

    pub unsafe fn unmap_page(virt_addr: VirtAddr) -> Result<(), PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let pte = pml.get_entry_specific(virt_addr, PAGING_LEVEL - 1)?;

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

        // Entry -> physical address -> virtual address -> valid &mut PageTable
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
        #[allow(static_mut_refs)]
        let phys_addr = unsafe { crate::mem::pmm::BUMP_ALLOCATOR.allocate_any(1, 1) }
            .map_err(|e| PagingError::AllocationError(e))?;

        let ret = unsafe {
            let ptr = core::ptr::without_provenance_mut(phys_addr.add_hhdm_offset().0);
            crate::utils::memset(ptr, 0, 0x1000);
            // TODO: Change this error to something more meaningfull
            (ptr as *mut PageTable)
                .as_mut()
                .ok_or(PagingError::EntryToPageTableFailed)?
        };

        Ok((ret, phys_addr))
    }
}

// TODO:
// flush tlb
