//! A simple spinlock implementation

use core::cell::UnsafeCell;
use core::hint;
use core::sync::atomic::{AtomicBool, Ordering};

/// A simple spinlock implementation
pub struct Spinlock<T> {
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
        while self.lock.swap(true, Ordering::Acquire) {
            hint::spin_loop();
        }
    }

    /// Release the spinlock
    #[inline(always)]
    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}
