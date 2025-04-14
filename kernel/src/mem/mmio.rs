// TODO: Write this better

use super::VirtAddr;

// #[derive(Debug)]
// pub struct RoReg<T>(*const T);
//
// impl<T> RoReg<T> {
//     #[inline]
//     pub const unsafe fn new(addr: VirtAddr) -> Self {
//         Self(core::ptr::without_provenance(addr.0))
//     }
//
//     #[inline]
//     pub unsafe fn read(&self) -> T {
//         unsafe { core::ptr::read_volatile(self.0) }
//     }
//
//     #[inline]
//     pub fn addr(&self) -> VirtAddr {
//         VirtAddr(self.0.addr())
//     }
// }

#[derive(Debug)]
pub struct RwReg<T>(*mut T);

impl<T> RwReg<T> {
    #[inline]
    pub const unsafe fn new(addr: VirtAddr) -> Self {
        Self(core::ptr::without_provenance_mut(addr.0))
    }

    #[inline]
    pub unsafe fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(self.0) }
    }

    #[inline]
    pub unsafe fn write(&self, value: T) {
        unsafe { core::ptr::write_volatile(self.0, value) }
    }

    // #[inline]
    // pub fn addr(&self) -> VirtAddr {
    //     VirtAddr(self.0.addr())
    // }
}
//
// #[derive(Debug)]
// pub struct WoReg<T>(*mut T);
//
// impl<T> WoReg<T> {
//     #[inline]
//     pub const unsafe fn new(addr: VirtAddr) -> Self {
//         Self(core::ptr::without_provenance_mut(addr.0))
//     }
//
//     #[inline]
//     pub unsafe fn write(&self, value: T) {
//         unsafe { core::ptr::write_volatile(self.0, value) }
//     }
//
//     #[inline]
//     pub fn addr(&self) -> VirtAddr {
//         VirtAddr(self.0.addr())
//     }
// }
