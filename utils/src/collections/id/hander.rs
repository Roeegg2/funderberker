//! Simple wrapper for an ID bump allocator

use super::Id;

/// A simple ID allocator that just hands out IDs from a virtually infinite pool, not requiring a
/// free
pub struct IdHander {
    /// The next ID to be allocated
    next: Id,
}

impl IdHander {
    /// Creates a new `IdHander` starting from the given ID.
    ///
    /// SAFETY: If an ID that is too big is picked and many handouts will be performed, then
    /// `next` will wrap over and that's an error
    #[inline]
    pub const unsafe fn new_starting_from(start_id: Id) -> Self {
        Self { next: start_id }
    }

    /// Creates a new `IdHander` starting from 0.
    #[inline]
    pub const fn new() -> Self {
        unsafe { Self::new_starting_from(Id(0)) }
    }

    /// Handout the next ID
    #[inline]
    pub fn handout(&mut self) -> Id {
        let ret = self.next;
        self.next = Id(self.next.0 + 1);

        ret
    }

    /// Get
    #[inline]
    pub const fn next(&self) -> Id {
        self.next
    }
}
