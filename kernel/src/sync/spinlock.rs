//! A simple spinlock implementation

use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, Ordering};

/// A simple spinlock implementation
pub struct Spinlock<T: ?Sized> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for Spinlock<T> {}
unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Create a new spinlock with the given data
    pub fn new(data: T) -> Self {
        Spinlock {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Spin until you can lock the spinlock, then lock it
    #[inline(always)]
    pub fn lock(&self) {
        loop {
            // Tell the processor we're spinning so it can optimize some stuff
            hint::spin_loop();

            if !self.lock.swap(true, Ordering::Acquire) {
                break;
            } 
        }
    }

    /// Release the spinlock
    #[inline(always)]
    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}
