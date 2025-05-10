//! A simple spinlock implementation

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use utils::spin_until;

/// A trait for types that can be used with the spinlock
///
/// SAFETY: This trait is unsafe because it CANNOT be implemented for non custom types.
pub trait SpinLockDropable: Send + Sync {
    /// Additional cleanup code for the spinlock, that will be called **BEFORE** the lock is
    /// released.
    /// NOTE: There is no need to release the lock here, it will be released for you. This simply an option for when you need to
    /// run some code before the lock is released.
    fn custom_unlock(&mut self) {}
}

// TODO: Break this into `mut`Gand non `mut` versions
/// A simple spinlock implementation
#[derive(Debug)]
pub struct SpinLock<T>
where
    T: SpinLockDropable,
{
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

/// A guard for the spinlock, which unlocks the spinlock when dropped
#[derive(Debug)]
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
    #[inline]
    pub fn lock(&self) -> SpinLockGuard<T> {
        spin_until!(!self.lock.swap(true, Ordering::Acquire));

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
            self.lock.unlock();
        };
    }
}

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

// ---- IMPLEMENTING SpinLockDropable for some common primitive types ----

impl SpinLockDropable for () {}
impl SpinLockDropable for i8 {}
impl SpinLockDropable for i16 {}
impl SpinLockDropable for i32 {}
impl SpinLockDropable for i64 {}
impl SpinLockDropable for i128 {}
impl SpinLockDropable for u8 {}
impl SpinLockDropable for u16 {}
impl SpinLockDropable for u32 {}
impl SpinLockDropable for u64 {}
impl SpinLockDropable for u128 {}
impl SpinLockDropable for isize {}
impl SpinLockDropable for usize {}
