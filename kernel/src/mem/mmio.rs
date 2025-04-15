use super::VirtAddr;

#[derive(Debug)]
pub struct RwReg<T>(*mut T);

impl<T> RwReg<T> {
    #[inline]
    pub const unsafe fn new(addr: VirtAddr) -> Self {
        Self(addr.0 as *mut T)
    }

    #[inline]
    pub unsafe fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(self.0) }
    }

    #[inline]
    pub unsafe fn write(&self, value: T) {
        unsafe { core::ptr::write_volatile(self.0, value) }
    }
}
