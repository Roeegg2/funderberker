//! Physical Memory Manager (PMM) module

#![cfg_attr(not(test), no_std)]
#![feature(box_vec_non_null)]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

#[cfg(feature = "limine")]
use limine::memory_map;

use utils::mem::PhysAddr;
use utils::sync::spinlock::{SpinLockGuard, SpinLockable};

extern crate alloc;

// TODO: Move this somewhere else
const BASIC_PAGE_SIZE: usize = 0x1000; // 4KB page size

mod buddy;

/// Errors that the PMM might encounter
#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PmmError {
    /// The requested block is out of bounds
    OutOfBounds,
    /// No available block of the requested size
    NoAvailableBlock,
    /// The requested block is already free
    FreeOfAlreadyFree,
    /// The requested alignment is invalid
    InvalidAlignment,
    /// The requested address is invalid
    InvalidAddress,
    /// The requested page count is invalid (0)
    EmptyAllocation,
    /// The requested page count is invalid (0)
    EmptyFree,
    /// The requested page count is too big
    TooBigAllocation,
}

/// Get the used PMM
pub fn get<'a>() -> SpinLockGuard<'a, impl PmmAllocator> {
    buddy::PMM.lock()
}

/// Initilizes the used PMM from limine
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine<'a>(
    mem_map: &'a [&'a memory_map::Entry],
) -> &'a limine::memory_map::Entry {
    let ret = unsafe { buddy::BuddyAllocator::init_from_limine(mem_map) };

    logger::info!("PMM initialized successfully");

    ret
}

pub trait PmmAllocator: SpinLockable {
    /// Tries to allocates a **physically** contiguious block of pages of size `page_count`
    /// which satisfy the passed `alignment` page alignment.
    /// If allocation if successfull, the physical address of the start of the block is returned.
    ///
    /// NOTE: `alignment should be passed as page granularity. (e.g. 1 for 4KB, 2 for 8KB, etc.)`
    #[must_use = "Not freeing allocated memory will leak it"]
    fn allocate(&mut self, alignment: usize, page_count: usize) -> Result<PhysAddr, PmmError>;

    /// Tries to allocate a **physically** contiguous block of memory at a specific address
    #[allow(dead_code)]
    fn allocate_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError>;

    /// Tries to free a contiguous block of pages.
    #[allow(dead_code)]
    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError>;

    /// Returns true if a page if free, false if it's not. If an error is encountered, an error is
    /// returned instead.
    #[allow(dead_code)]
    fn is_page_free(&self, addr: PhysAddr, page_count: usize) -> Result<bool, PmmError>;

    /// Initilizes the PMM when using Limine using limine's memory map.
    #[cfg(feature = "limine")]
    unsafe fn init_from_limine<'a>(
        mem_map: &'a [&'a memory_map::Entry],
    ) -> &'a limine::memory_map::Entry;
}

/// Get the maximum addressable page count from the memory map.
/// This is done by finding the last memory map entry that is usable and calculating the page count
#[cfg(feature = "limine")]
fn get_page_count_from_mem_map(mem_map: &[&memory_map::Entry]) -> usize {
    let last_descr = mem_map
        .iter()
        .rev()
        .find(|&entry| {
            matches!(
                entry.entry_type,
                memory_map::EntryType::USABLE
                    | memory_map::EntryType::BOOTLOADER_RECLAIMABLE
                    | memory_map::EntryType::ACPI_RECLAIMABLE
                    | memory_map::EntryType::EXECUTABLE_AND_MODULES
            )
        })
        .unwrap();

    (last_descr.base + last_descr.length) as usize / BASIC_PAGE_SIZE
}
