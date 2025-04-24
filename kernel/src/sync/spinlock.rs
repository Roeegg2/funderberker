//! A simple spinlock implementation

use core::cell::UnsafeCell;
use core::hint;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub trait SpinLockDropable {
    /// Additional cleanup code for the spinlock, that will be called **BEFORE** the lock is
    /// released.
    /// NOTE: There is no need to release the lock here, it will be released for you. This simply an option for when you need to
    /// run some code before the lock is released.
    unsafe fn custom_unlock(&mut self) {}
}

/// A simple spinlock implementation
pub struct SpinLock<T>
where
    T: SpinLockDropable,
{
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

/// A guard for the spinlock, which unlocks the spinlock when dropped
pub struct SpinLockGuard<'a, T>
where
    T: SpinLockDropable,
{
    lock: &'a SpinLock<T>,
    data: &'a mut T,
}

unsafe impl<T: Send + SpinLockDropable> Send for SpinLock<T> {}
unsafe impl<T: Send + SpinLockDropable> Sync for SpinLock<T> {}

impl<T> SpinLock<T>
where
    T: SpinLockDropable,
{
    /// Create a new spinlock with the given data
    pub const fn new(data: T) -> Self {
        SpinLock {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Spin until you can lock the spinlock, then lock it
    pub fn lock(&self) -> SpinLockGuard<T> {
        loop {
            // Tell the processor we're spinning so it can optimize some stuff
            hint::spin_loop();

            if !self.lock.swap(true, Ordering::Acquire) {
                break;
            }
        }

        SpinLockGuard {
            lock: self,
            data: unsafe { self.data.get().as_mut().unwrap() },
        }
    }

    /// Release the spinlock
    unsafe fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

impl<T> Drop for SpinLockGuard<'_, T>
where
    T: SpinLockDropable,
{
    fn drop(&mut self) {
        unsafe {
            // Run some custom unlock functionality if there is any
            self.data.custom_unlock();

            // Now unlock the spinlock
            self.lock.unlock()
        };
    }
}

// Deref to access the underlying data
impl<T> Deref for SpinLockGuard<'_, T>
where
    T: SpinLockDropable,
{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T>
where
    T: SpinLockDropable,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}
