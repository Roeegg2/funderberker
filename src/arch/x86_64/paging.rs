//! The core x86_64 paging mechanism

#[cfg(all(feature = "paging_4", feature = "paging_5"))]
compiler_error!("Can't have both 4 level and 5 level paging. Choose one of the options");
#[cfg(not(any(feature = "paging_4", feature = "paging_5")))]
compiler_error!("No paging level is selected. Choose one of the options");

use crate::{
    mem::{PhysAddr, VirtAddr, pmm::PmmError,},
    read_cr,
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
    /// Encountered an invalid virtual address during translation at level _
    InvalidVirtualAddress(u8),
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
    /// All possible flags turned on
    const FLAG_ALL: usize = 0b0111_1111_1111;

    /// The following flags' meanings are for our use, so I've given them my own meaning:
    const FLAG_AVL: usize = 0b111 << 9;
}

impl Entry {
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

    fn allocate_table_entry(&mut self) -> Result<VirtAddr, PagingError> {
        // TODO: Memset to 0?
        #[allow(static_mut_refs)]
        let next_table_page = unsafe { crate::mem::pmm::BUMP_ALLOCATOR.allocate_any(1, 1) }
            .map_err(|e| PagingError::AllocationError(e))?;

        // Set the entry
        *(self) = Entry(next_table_page.0);
        self.set(Entry::FLAG_P);

        let virt_addr: VirtAddr = PhysAddr::from(*self).add_hhdm_offset();
        Ok(virt_addr)
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
            // Extract the phys addr outside of `value`, add HHDM offset so it's a valid VMM virtual
            // address
            let ptr = core::ptr::without_provenance_mut::<PageTable>(
                PhysAddr::from(value).add_hhdm_offset().0,
            );
            ptr.as_mut().ok_or(PagingError::EntryToPageTableFailed)
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable([Entry; 512]);

// LATETODO: Try to TCE optimize to each of the recursive function when it's supported by Rust
impl PageTable {
    /// Initilize paging
    /// NOTE: SHOULD ONLY BE CALLED ONCE PRETTY EARLY AT BOOT!
    pub unsafe fn init() {
        //#[allow(static_mut_refs)]
        //let pml = unsafe {crate::mem::pmm::BUMP_ALLOCATOR.allocate_any(1, 1)};
    }

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
        let (pte, virt_addr) = pml.get_create_pte_any(false, PAGING_LEVEL - 1)?;
        pte.set(phys_addr.0 | flags);

        Ok(virt_addr)
    }

    // Tries to map the given physical address to the given virtual address.
    pub fn map_page_specific(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        flags: usize,
    ) -> Result<(), PagingError> {
        let pml = unsafe { PageTable::get_pml() }?;
        let pte = pml.get_create_pte_specific(VirtAddr(virt_addr.0 >> 12), PAGING_LEVEL - 1)?;
        pte.set(phys_addr.0 | flags);

        Ok(())
    }

    // Tries to unmap the given virtual address
    pub unsafe fn unmap_page(&mut self, virt_addr: VirtAddr) -> Result<(), PagingError> {
        let pte = self.free_pte_path(virt_addr, PAGING_LEVEL - 1)?;
        pte.unset_flags(Entry::FLAG_P);

        Ok(())
    }

    // TODO: Add a mechanism to keep track of whether we should free the paging structures or
    // rather keep them in memory for a near allocation
    /// Tries to get the PTE of an **already available** virtual address (ie one that has all of it's tables in memory).
    /// NOTE: Make sure the entry isn't used before doing any modifications to it!
    fn free_pte_path(&mut self, virt_addr: VirtAddr, level: u8) -> Result<&mut Entry, PagingError> {
        // NOTE: Because of the bitwise and, next index can't be more than 511 so it's safe to
        // index using it without checks
        let index = virt_addr.0 & 0b1_1111_1111;
        if level == 0 {
            return Ok(&mut self.0[index]);
        }

        if !self.0[index].get_flags(Entry::FLAG_P) {
            return Err(PagingError::MissingPagingTable(level));
        }

        // Entry -> physical address -> virtual address -> valid &mut PageTable
        let next_table: &mut PageTable = self.0[index].try_into()?;
        next_table.free_pte_path(VirtAddr(virt_addr.0 >> 9), level - 1)
    }

    /// Tries to get the PTE of a virtual address. Will allocate any tables if it needs to
    /// to make given virtual address addressable
    /// NOTE: Make sure the entry isn't used before doing any modifications to it!
    fn get_create_pte_specific(
        &mut self,
        virt_addr: VirtAddr,
        level: u8,
    ) -> Result<&mut Entry, PagingError> {
        // NOTE: Because of the bitwise and, next index can't be more than 511 so it's safe to
        // index using it without checks
        let index = virt_addr.0 & 0b1_1111_1111;
        if level == 0 {
            return Ok(&mut self.0[index]);
        }

        if !self.0[index].get_flags(Entry::FLAG_P) {
            self.0[index].allocate_table_entry()?;
        }

        let next_table: &mut PageTable = self.0[index].try_into()?;
        next_table.get_create_pte_specific(VirtAddr(virt_addr.0 >> 9), level - 1)
    }

    /// Tries to get the PTE of any available address.
    fn get_create_pte_any(
        &mut self,
        mut newly_mapped: bool,
        level: u8,
    ) -> Result<(&mut Entry, VirtAddr), PagingError> {
        // If we reached PTE
        if level == 0 {
            // Find the first non present (ie not taken) entry
            let (index, entry) = self
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
        let entry: (usize, &mut Entry);
        if !newly_mapped
            && let Some(value) = self
                .0
                .iter_mut()
                .enumerate()
                .find(|pte| pte.1.get_flags(Entry::FLAG_P) && !pte.1.get_flags(Entry::FLAG_AVL))
        {
            entry = value;
            // If user specified we should create the table if it's missing
        } else {
            // Find an unused entry in the current table
            entry = self
                .0
                .iter_mut()
                .enumerate()
                .find(|pte| !pte.1.get_flags(Entry::FLAG_P))
                .ok_or(PagingError::UnexpectedlyTableFull(level))?;

            // Allocate a new entry
            entry.1.allocate_table_entry()?;
            newly_mapped = true;
        }

        let next_table: &mut PageTable = (*entry.1).try_into()?;
        // Go to next level
        next_table
            .get_create_pte_any(newly_mapped, level - 1)
            .map(|mut ret| {
                // Construct next index in virtual address
                ret.1 = VirtAddr((ret.1.0 << 9) | entry.0);

                ret
            })
    }
}

// TODO:
// flush tlb
