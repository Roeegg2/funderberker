use core::marker;

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

pub trait MmioReg {
    fn offset(self) -> usize;
}

#[derive(Debug)]
pub struct MmioArea<R, W, T>
where
    R: MmioReg,
    W: MmioReg,
{
    base: *mut T,
    _writable: marker::PhantomData<W>,
    _readable: marker::PhantomData<R>,
}

impl<R, W, T> MmioArea<R, W, T>
where
    R: MmioReg,
    W: MmioReg,
{
    #[inline]
    pub const fn new(base: *mut T) -> Self {
        Self {
            base,
            _writable: marker::PhantomData,
            _readable: marker::PhantomData,
        }
    }

    #[inline]
    pub unsafe fn read(&self, reg: R) -> T {
        unsafe { core::ptr::read_volatile(self.base.add(reg.offset())) }
    }

    #[inline]
    pub unsafe fn write(&self, reg: W, value: T) {
        unsafe { core::ptr::write_volatile(self.base.add(reg.offset()), value) }
    }
}
