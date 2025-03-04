pub mod pmm;

pub static mut HHDM_OFFSET: usize = 0;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

impl PhysAddr {
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
