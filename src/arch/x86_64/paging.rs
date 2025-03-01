//! The core x86_64 paging mechanism

use crate::{
    mem::{PhysAddr, VirtAddr, pmm::PmmError},
    read_cr,
};

#[cfg(all(feature = "paging_4", feature = "paging_5"))]
compiler_error!("Can't have both 4 level and 5 level paging. Choose one of the options");
#[cfg(not(any(feature = "paging_4", feature = "paging_5")))]
compiler_error!("No paging level is selected. Choose one of the options");

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
    /// Encountered an invalid virtual address during translation at level _
    InvalidVirtualAddress(u8),
    /// One (or possibly more) paging tables were found missing during virtual address translation
    /// at level _
    MissingPagingTable(u8),
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct Entry(usize);

impl Entry {
    /// Present bit - 0 => not present. 1 => present
    const FLAG_P: usize = 1 << 0;
    /// Uead/write - 0 => just read. 1 => read + write
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
    const FLAG_AVL: usize = 0b111 << 9;
    /// All possible flags turned on
    const FLAG_ALL: usize = 0b0111_1111_1111;

    // Set address or flags to entry
    const fn set(&mut self, data: usize) {
        self.0 |= data;
    }

    const fn unset_flags(&mut self, flags: usize) {
        self.0 &= !flags
    }

    const fn get_flags(&self, flag: usize) -> bool {
        (self.0 | flag) != 0
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
            let ptr = core::ptr::without_provenance_mut::<PageTable>(PhysAddr::from(value).0);
            ptr.as_mut().ok_or(PagingError::EntryToPageTableFailed)
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
struct PageTable([Entry; 512]);

impl PageTable {
    // NOTE: Not sure whether this should be static or not...
    #[inline]
    unsafe fn get_pml() -> Result<&'static mut PageTable, PagingError> {
        let addr = Entry(read_cr!(cr3));
        addr.try_into()
    }
}

pub fn map_page(flags: usize) -> Result<VirtAddr, PagingError> {
    let (pte, virt_addr) = unsafe {get_pte_any()}?;
    pte.set(virt_addr.0 | flags);

    Ok(virt_addr)
}

pub fn map_page_to(virt_addr: VirtAddr, flags: usize) -> Result<(), PagingError> {
    let pte = unsafe {get_pte_specific(virt_addr)}?;
    pte.set(virt_addr.0 | flags);

    Ok(())
}

/// Tries to get the PTE associated with `virt_addr`, and returns a reference to said entry if
/// found.
/// NOTE: Make sure the entry isn't used before doing any modifications to it!
unsafe fn get_pte_specific<'a>(virt_addr: VirtAddr) -> Result<&'a mut Entry, PagingError> {
    fn actual_impl(table: &mut PageTable, virt_addr: VirtAddr, level: u8,) -> Result<&mut Entry, PagingError> {
        let index = virt_addr.0 & 0b1_1111_1111;
        if index >= 511 {
            return Err(PagingError::InvalidVirtualAddress(level));
        }

        if level == 0 {
            return Ok(&mut table.0[index]);
        }

        // TODO: Need to zero out table?
        // Allocate physical page for the table
        #[allow(static_mut_refs)]
        let next_table_page = unsafe { crate::mem::pmm::BUMP_ALLOCATOR.allocate_any(1, 1) }
            .map_err(|e| PagingError::AllocationError(e))?;

        // PhysAddr -> Entry -> PageTable
        let next_table = Entry(next_table_page.0).try_into()?;
        actual_impl(next_table, VirtAddr(virt_addr.0 >> 9), level-1)
    }

    let pml = unsafe { PageTable::get_pml() }?;
    actual_impl(pml,VirtAddr(virt_addr.0 >> 12), PAGING_LEVEL - 1)
}

// TODO: Make this shit cleaner
/// Tries to find some available PTE entry. If it doesn't find one, it creates one. If it
/// can't create one, well... Unlucky :)
unsafe fn get_pte_any<'a>() -> Result<(&'a mut Entry, VirtAddr), PagingError> {
    fn actual_impl(
        table: &mut PageTable,
        level: u8,
        newly_mapped: bool,
    ) -> Result<(&mut Entry, VirtAddr), PagingError> {
        // If we reached PTE
        if level == 0 {
            // Find the first non present (ie not taken) entry
            let (index, entry) = table
                .0
                .iter_mut()
                .enumerate()
                .find(|pte| !pte.1.get_flags(Entry::FLAG_P))
                .ok_or(PagingError::UnexpectedlyTableFull(0))?;

            // Return mutable ref to entry, and the index used
            return Ok((entry, VirtAddr(index)));
        }

        // If we didn't map a new table previously, and there's an available entry in the current
        // table
        if !newly_mapped
            && let Some(entry) = table
                .0
                .iter_mut()
                .enumerate()
                .find(|pte| pte.1.get_flags(Entry::FLAG_P) && !pte.1.get_flags(Entry::FLAG_AVL))
        {
            return actual_impl((*entry.1).try_into()?, level - 1, false).map(
                |mut ret| {
                    // If last index was 511 (ie. The table is full), mark its entry as full
                    if (ret.1.0 & 511) == 511 {
                        entry.1.set(Entry::FLAG_AVL);
                    }

                    // Construct next index in virtual address
                    ret.1 = VirtAddr((ret.1.0 << 9) | entry.0);

                    ret
                },
            );
        }

        // If we don't have any present and not full tables left, find a isn't present
        let entry = table
            .0
            .iter_mut()
            .enumerate()
            .find(|pte| !pte.1.get_flags(Entry::FLAG_G))
            .ok_or(PagingError::UnexpectedlyTableFull(level))?;

        // TODO: Need to zero out table?
        // Allocate physical page for the table
        #[allow(static_mut_refs)]
        let next_table_page = unsafe { crate::mem::pmm::BUMP_ALLOCATOR.allocate_any(1, 1) }
            .map_err(|e| PagingError::AllocationError(e))?;

        // PhysAddr -> Entry -> PageTable
        let next_table = Entry(next_table_page.0).try_into()?;

        actual_impl(next_table, level - 1, true).map(|mut ret| {
            // If last index was 511 (ie. The table is full), mark its entry as full
            if (ret.1.0 & 511) == 511 {
                entry.1.set(Entry::FLAG_AVL);
            }

            // Construct next index in virtual address
            ret.1 = VirtAddr((ret.1.0 << 9) | entry.0);

            ret
        })
    }

    let pml = unsafe { PageTable::get_pml() }?;
    actual_impl(pml, PAGING_LEVEL - 1, false)
}

// TODO:
// unmap pages
// flush tlb
