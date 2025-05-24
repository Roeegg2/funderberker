//! Memory management and abstraction layer over arch specific details
use core::{
    ops::{Add, Sub},
    ptr::NonNull,
};

use alloc::fmt;

#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::paging::PageSize;

use utils::collections::fast_lazy_static::FastLazyStatic;

pub mod mmio;
pub mod pmm;
pub mod slab;
pub mod vmm;

// TODO: Make this uninit instead of 0?
// TODO: Make this a SetOnce
/// An temporary invalid HHDM offset that will be changed once the HHDM offset is set
const INVALID_HHDM_OFFSET: usize = 0xFFFF_FFFF_FFFF_FFFF;

pub static HHDM_OFFSET: FastLazyStatic<usize> = FastLazyStatic::new(INVALID_HHDM_OFFSET);

/// A physical address
#[repr(transparent)]
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

/// A virtual address
#[repr(transparent)]
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    /// Get the physical address of a virtual address **that is HHDM mapped**
    ///
    /// NOTE: This function can't be const since we don't know the HHDM offset at compile time
    pub fn subtract_hhdm_offset(self) -> PhysAddr {
        PhysAddr(self.0 - HHDM_OFFSET.get())
    }

    #[cfg(target_arch = "x86_64")]
    pub const fn next_level_index(self, level: usize) -> usize {
        assert!(level < 5);

        (self.0 >> (PageSize::Size4KB.offset_size() + (level * 9))) & 0b1_1111_1111
    }
}

impl PhysAddr {
    /// Get the virtual address of a physical address. A Virtual address **that is HHDM mapped**
    pub fn add_hhdm_offset(self) -> VirtAddr {
        VirtAddr(self.0 + HHDM_OFFSET.get())
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#x}", self.0) // Formats as hex with "0x" prefix
    }
}

impl fmt::Debug for PhysAddr {
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

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Add<VirtAddr> for VirtAddr {
    type Output = usize;

    fn add(self, rhs: VirtAddr) -> Self::Output {
        self.0 + rhs.0
    }
}

impl Add<PhysAddr> for PhysAddr {
    type Output = usize;

    fn add(self, rhs: PhysAddr) -> Self::Output {
        self.0 + rhs.0
    }
}

impl Sub<usize> for VirtAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Sub<usize> for PhysAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl Sub<VirtAddr> for VirtAddr {
    type Output = usize;

    fn sub(self, rhs: VirtAddr) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<PhysAddr> for PhysAddr {
    type Output = usize;

    fn sub(self, rhs: PhysAddr) -> Self::Output {
        self.0 - rhs.0
    }
}

// NOTE: The following two implementations are safe, since this operation cannot generate UB.
// BUT using the resulting pointers is obviously unsafe, so be careful!
impl<T> From<VirtAddr> for *const T {
    fn from(value: VirtAddr) -> Self {
        value.0 as *const T
    }
}

impl<T> From<VirtAddr> for *mut T {
    fn from(value: VirtAddr) -> Self {
        value.0 as *mut T
    }
}

impl<T> TryFrom<VirtAddr> for NonNull<T> {
    type Error = ();

    fn try_from(value: VirtAddr) -> Result<Self, Self::Error> {
        NonNull::new(value.0 as *mut T).ok_or(())
    }
}
