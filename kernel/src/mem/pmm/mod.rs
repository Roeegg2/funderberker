use bump::BumpAllocator;

use super::PhysAddr;

mod bump;

/// Errors that the PMM might encounter
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PmmError {
    OutOfBounds,
    NoAvailableBlock,
    FreeOfAlreadyFree,
    InvalidAlignment,
    InvalidAddress,
}

/// Get the used PMM
pub fn get() -> &'static mut impl PmmAllocator {
    #[cfg(feature = "pmm_bump")]
    #[allow(static_mut_refs)]
    unsafe {
        &mut bump::BUMP_ALLOCATOR
    }
}

/// Initilizes the used PMM from limine
#[cfg(feature = "limine")]
pub unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
    #[cfg(feature = "pmm_bump")]
    unsafe {
        BumpAllocator::init_from_limine(mem_map)
    };
}

pub trait PmmAllocator {
    /// Tries to allocates a contiguious block of pages of size `page_count` which satisfy the passed `alignment`. If allocation if successfull, the physical address of the start of the block is returned.
    fn alloc_any(&mut self, alignment: usize, page_count: usize) -> Result<PhysAddr, PmmError>;

    /// Tries to allocate a contiguous block of memory at a specific address
    #[allow(dead_code)]
    fn alloc_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError>;

    /// Tries to free a contiguous block of pages.
    #[allow(dead_code)]
    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError>;

    /// Returns true if a page if free, false if it's not. If an error is encountered, an error is
    /// returned.
    #[allow(dead_code)]
    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, PmmError>;

    /// Initilizes the PMM when using Limine using limine memory map.
    #[cfg(feature = "limine")]
    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]);
}
