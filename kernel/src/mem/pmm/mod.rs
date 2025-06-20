//! Physical Memory Manager (PMM) module

use core::num::NonZero;

use crate::sync::spinlock::{SpinLockGuard, SpinLockable};

use super::PhysAddr;

#[cfg(feature = "pmm_buddy")]
mod buddy;
#[cfg(feature = "pmm_bump")]
mod bump;

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
}

/// Get the used PMM
pub fn get<'a>() -> SpinLockGuard<'a, impl PmmAllocator> {
    #[cfg(feature = "pmm_bump")]
    {
        bump::BUMP_ALLOCATOR.lock()
    }
    #[cfg(feature = "pmm_buddy")]
    {
        buddy::BUDDY_ALLOCATOR.lock()
    }
}

/// Initilizes the used PMM from limine
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
    unsafe {
        #[cfg(feature = "pmm_bump")]
        bump::BumpAllocator::init_from_limine(mem_map);
        #[cfg(feature = "pmm_buddy")]
        buddy::BuddyAllocator::init_from_limine(mem_map);
    };
}

pub trait PmmAllocator: SpinLockable {
    /// Tries to allocates a **physically** contiguious block of pages of size `page_count`
    /// which satisfy the passed `alignment` page alignment.
    /// If allocation if successfull, the physical address of the start of the block is returned.
    ///
    /// NOTE: `alignment should be passed as page granularity. (e.g. 1 for 4KB, 2 for 8KB, etc.)`
    fn allocate(
        &mut self,
        alignment: NonZero<usize>,
        page_count: NonZero<usize>,
    ) -> Result<PhysAddr, PmmError>;

    /// Tries to allocate a **physically** contiguous block of memory at a specific address
    #[allow(dead_code)]
    fn allocate_at(&mut self, addr: PhysAddr, page_count: NonZero<usize>) -> Result<(), PmmError>;

    /// Tries to free a contiguous block of pages.
    #[allow(dead_code)]
    unsafe fn free(&mut self, addr: PhysAddr, page_count: NonZero<usize>) -> Result<(), PmmError>;

    /// Returns true if a page if free, false if it's not. If an error is encountered, an error is
    /// returned instead.
    #[allow(dead_code)]
    fn is_page_free(&self, addr: PhysAddr, page_count: NonZero<usize>) -> Result<bool, PmmError>;

    /// Initilizes the PMM when using Limine using limine's memory map.
    #[cfg(feature = "limine")]
    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]);
}
