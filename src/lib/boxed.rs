// A minimal Box implementation for kernel use
// Memory safety without the standard library

use core::ptr::NonNull;
use core::alloc::{Layout, GlobalAlloc};
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::fmt;
use core::mem;

use super::allocator::KERNEL_HEAP_ALLOCATOR;

pub struct Box<T> {
    ptr: NonNull<T>,
    _marker: PhantomData<T>,
}

impl<T> Box<T> {
    /// Allocate memory for a single value of type T and initialize it
    pub fn new(value: T) -> Self {
        unsafe {
            // Allocate memory for the value
            let layout = Layout::new::<T>();
            let ptr = KERNEL_HEAP_ALLOCATOR.alloc(layout) as *mut T;

            if ptr.is_null() {
                panic!("Failed to allocate memory");
            }

            // Initialize the allocated memory with the provided value
            ptr.write(value);

            Box {
                ptr: NonNull::new_unchecked(ptr),
                _marker: PhantomData,
            }
        }
    }

    /// Consumes the Box and returns the raw pointer
    pub fn into_raw(b: Self) -> *mut T {
        let ptr = b.ptr.as_ptr();
        mem::forget(b); // Prevent Drop from being called
        ptr
    }

    /// Reconstruct a Box from a raw pointer
    pub unsafe fn from_raw(ptr: *mut T) -> Self {
        Box {
            ptr: unsafe {NonNull::new_unchecked(ptr)},
            _marker: PhantomData,
        }
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            // Deallocate the memory
            let layout = Layout::new::<T>();
            KERNEL_HEAP_ALLOCATOR.dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Box({:?})", &**self)
    }
}

//impl<T> Box<[T]> {
//    /// Create a boxed slice
//    pub fn new_slice(len: usize) -> Self {
//        unsafe {
//            let layout = Layout::array::<T>(len).unwrap();
//            let ptr = alloc::alloc::alloc(layout) as *mut T;
//
//            if ptr.is_null() {
//                panic!("Failed to allocate memory");
//            }
//
//            Box {
//                ptr: NonNull::new_unchecked(ptr),
//                _marker: PhantomData,
//            }
//        }
//    }
//}
