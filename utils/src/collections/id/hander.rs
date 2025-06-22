//! Simple wrapper for an ID bump allocator

use crate::sync::spinlock::SpinLockable;

use super::Id;

/// A simple ID allocator that just hands out IDs from a virtually infinite pool, not requiring a
/// free
pub struct IdHander {
    /// The next ID to be allocated
    next: Id,
    /// The maximum ID that can be allocated.
    ///
    /// The allocator will panic once `next` exceeds this value.
    max: Id,
}

impl IdHander {
    // TODO: Remove this and use the `Default` when const default is possible
    /// Return an uninitialized `IdHander`.
    pub const fn uninit() -> Self {
        Self {
            next: Id(0),
            max: Id(0),
        }
    }

    /// Creates a new `IdHander` starting from the given ID.
    #[inline]
    pub const fn new_starting_from(start_id: Id, max_id: Id) -> Self {
        Self {
            next: start_id,
            max: max_id,
        }
    }

    /// Creates a new `IdHander` starting from 0.
    #[inline]
    pub const fn new(max_id: Id) -> Self {
        Self::new_starting_from(Id(0), max_id)
    }

    /// Handout the next ID
    #[inline]
    pub fn handout(&mut self) -> Id {
        unsafe { self.handout_and_skip(1) }
    }

    /// Skips the next `count`
    #[inline]
    pub unsafe fn skip(&mut self, count: usize) {
        self.next = Id(self.next.0 + count);
    }

    /// Handout the next ID and skip `count` IDs
    #[inline]
    pub unsafe fn handout_and_skip(&mut self, count: usize) -> Id {
        let ret = self.next;
        self.next = Id(self.next.0 + count);

        if self.next > self.max {
            panic!(
                "ID allocator has exceeded the maximum ID limit of {}",
                self.max.0
            );
        }

        ret
    }

    /// Get
    #[inline]
    pub const fn next(&self) -> Id {
        self.next
    }
}

impl SpinLockable for IdHander {}
