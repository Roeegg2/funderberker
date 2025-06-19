//! Wrappers for safer and easier handling of MMIO

use core::{
    marker::PhantomData,
    ptr::{read_volatile, write_volatile},
};

/// A trait for types that can be used as MMIO register offsets
///
/// NOTE: SHOULD NOT BE IMPLEMENTED FOR PRIMITIVE TYPES!
/// It's only implement for `usize` here.
pub trait Offsetable {
    /// Returns the offset **in bytes** of the register from the base address.
    fn offset(self) -> usize;
}

/// A wrapper for a single MMIO register
#[derive(Debug)]
pub struct MmioCell<T>
where
    T: Copy + Sized,
{
    base: *mut T,
}

/// A wrapper for a MMIO area. This is the same as `MmioCell`, but for a range of registers.
#[derive(Debug)]
pub struct MmioArea<R, W, T>
where
    R: Offsetable,
    W: Offsetable,
    T: Copy + Sized,
{
    _writable: PhantomData<W>,
    _readable: PhantomData<R>,
    base: *mut T,
}

impl<R, W, T> MmioArea<R, W, T>
where
    R: Offsetable,
    W: Offsetable,
    T: Copy + Sized,
{
    /// Creates a new `MmioArea` with the given base address.
    #[inline]
    pub const fn new(base: *mut T) -> Self {
        Self {
            base,
            _writable: PhantomData,
            _readable: PhantomData,
        }
    }

    /// Read an MMIO register in the area. `reg` should have `reg.offset()` return the offset *in
    /// bytes*
    #[inline]
    pub unsafe fn read(&self, reg: R) -> T {
        unsafe { read_volatile(self.base.byte_add(reg.offset())) }
    }

    /// Write to an MMIO register in the area reg` should have `reg.offset()` return the offset *in
    /// bytes*
    #[inline]
    pub unsafe fn write(&self, reg: W, value: T) {
        unsafe { write_volatile(self.base.byte_add(reg.offset()), value) }
    }

    /// Override the base address of the MMIO area
    #[inline]
    pub const unsafe fn change_base(&mut self, ptr: *mut T) {
        self.base = ptr;
    }

    /// Get the base address of the MMIO area
    #[inline]
    pub const fn base(&self) -> *mut T {
        self.base
    }
}

impl<T> MmioCell<T>
where
    T: Copy + Sized,
{
    /// Creates a new `MmioCell` with the given base address.
    #[inline]
    pub const fn new(base: *mut T) -> Self {
        Self { base }
    }

    /// Read an MMIO register in the area. `reg` should have `reg.offset()` return the offset **in
    /// bytes**
    #[inline]
    pub unsafe fn read(&self) -> T {
        unsafe { read_volatile(self.base) }
    }

    /// Write to an MMIO register in the area reg` should have `reg.offset()` return the offset **in
    /// bytes**
    #[inline]
    pub unsafe fn write(&self, value: T) {
        unsafe { write_volatile(self.base, value) }
    }
}

impl Offsetable for usize {
    fn offset(self) -> usize {
        self
    }
}
