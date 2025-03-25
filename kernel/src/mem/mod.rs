use core::ptr::NonNull;

use alloc::fmt;

use crate::arch::BASIC_PAGE_SIZE;

pub mod pmm;
pub mod vmm;

pub static mut HHDM_OFFSET: usize = 0;

#[repr(transparent)]
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    /// Get the physical address of a virtual address **that is HHDM mapped**
    pub fn subtract_hhdm_offset(self) -> PhysAddr {
        unsafe { PhysAddr(self.0 - HHDM_OFFSET) }
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#x}", self.0) // Formats as hex with "0x" prefix
    }
}

impl<T> From<*const T> for VirtAddr {
    fn from(value: *const T) -> Self {
        Self(value.addr())
    }
}

impl<T> From<*mut T> for VirtAddr {
    fn from(value: *mut T) -> Self {
        Self(value.addr())
    }
}
impl<T> From<NonNull<T>> for VirtAddr {
    fn from(value: NonNull<T>) -> Self {
        Self(value.as_ptr().addr())
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#x}", self.0) // Formats as hex with "0x" prefix
    }
}

impl PhysAddr {
    /// Get the virtual address of a physical address. A Virtual address **that is HHDM mapped**
    pub fn add_hhdm_offset(self) -> VirtAddr {
        unsafe { VirtAddr(self.0 + HHDM_OFFSET) }
    }
}

/// A page ID is a unique identifier for a page
pub(self) type PageId = usize;

/// Convert a page ID to a physical address
pub(self) fn page_id_to_addr(page_id: PageId) -> usize {
    page_id * BASIC_PAGE_SIZE
}

/// Convert a physical address to a page ID
pub(self) fn addr_to_page_id(addr: usize) -> Option<PageId> {
    if addr % BASIC_PAGE_SIZE != 0 {
        return None;
    }

    Some(addr / BASIC_PAGE_SIZE)
}
