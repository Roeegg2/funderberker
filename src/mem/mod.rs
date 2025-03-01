pub mod pmm;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

/// Subtract the HHDM offset of VMM to turn a virtual address into a physical one
/// ONLY VALID TO USE IN VMM SINCE THAT'S THE ONLY MEMORY MAP THAT IS DIRECT MAPPED!
impl VirtAddr {
    pub fn subtract_hhdm_offset(self) -> PhysAddr {
        unsafe { PhysAddr(self.0 - HHDM_OFFSET) }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

pub static mut HHDM_OFFSET: usize = 0;

impl PhysAddr {
    pub fn add_hhdm_offset(self) -> VirtAddr {
        unsafe { VirtAddr(self.0 + HHDM_OFFSET) }
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

pub(self) type PageId = usize;

pub(self) fn page_id_to_addr(page_id: PageId) -> usize {
    page_id * 4096
}

pub(self) fn addr_to_page_id(addr: usize) -> Option<PageId> {
    if addr % 4096 != 0 {
        return None;
    }

    Some(addr / 4096)
}
