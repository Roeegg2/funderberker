use super::VirtAddr;

#[derive(Debug)]
pub struct RwReg<T> (MmioReg<T>);

impl<T> RwReg<T> {
    #[inline]
    pub const unsafe fn new(addr: VirtAddr) -> Self {
        Self(MmioReg::new(addr))
    }

    #[inline]
    pub unsafe fn read(&self) -> T {
        self.0.read()
    }

    #[inline]
    pub unsafe fn write(&self, value: T) {
        self.0.write(value)
    }

    #[inline]
    pub const fn addr(&self) -> VirtAddr {
        self.0.addr()
    }
}

#[derive(Debug)]
pub struct RoReg<T> (MmioReg<T>);

impl<T> RoReg<T> {
    #[inline]
    pub const unsafe fn new(addr: VirtAddr) -> Self {
        Self(MmioReg::new(addr))
    }

    #[inline]
    pub unsafe fn read(&self) -> T {
        self.0.read()
    }

    #[inline]
    pub const fn addr(&self) -> VirtAddr {
        self.0.addr()
    }
}

struct MmioReg<T> {
    addr: VirtAddr,
    phantom: core::marker::PhantomData<T>,
}

impl<T> MmioReg<T> {
    #[inline]
    pub const unsafe fn new(addr: VirtAddr) -> Self {
        Self {
            addr,
            phantom: core::marker::PhantomData,
        }
    }

    #[inline]
    pub unsafe fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(self.addr.0 as *const T) }
    }

    #[inline]
    pub unsafe fn write(&self, value: T) {
        unsafe { core::ptr::write_volatile(self.addr.0 as *mut T, value) }
    }

    #[inline]
    pub const fn addr(&self) -> VirtAddr {
        self.addr
    }
}

