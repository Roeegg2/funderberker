use core::ptr::NonNull;

pub mod pmm;
pub mod vmm;

pub static mut HHDM_OFFSET: usize = 0;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    /// Get the physical address of a virtual address **that is HHDM mapped**
    pub fn subtract_hhdm_offset(self) -> PhysAddr {
        unsafe { PhysAddr(self.0 - HHDM_OFFSET) }
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
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

impl PhysAddr {
    /// Get the virtual address of a physical address. A Virtual address **that is HHDM mapped**
    pub fn add_hhdm_offset(self) -> VirtAddr {
        unsafe { VirtAddr(self.0 + HHDM_OFFSET) }
    }
}

pub(self) type PageId = usize;

pub(self) fn page_id_to_addr(page_id: PageId) -> usize {
    page_id * 0x1000
}

pub(self) fn addr_to_page_id(addr: usize) -> Option<PageId> {
    if addr % 0x1000 != 0 {
        return None;
    }

    Some(addr / 0x1000)
}
