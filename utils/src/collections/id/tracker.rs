use core::ops::Range;

use crate::collections::bitmap::Bitmap;

use super::Id;

/// Possible errors the ID allocator might encounter
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IdTrackerError {
    /// The allocator is out of IDs
    OutOfIds,
    /// The ID that was trying to be freed is already freed
    IdAlreadyFree,
    /// ID Out of bounds
    InvalidId,
    /// The ID that was trying to be allocated is already taken
    IdAlreadyTaken,
}

/// ID allocator that allocates and tracks IDs from a given, finite, pool.
pub struct IdTracker {
    /// Bitmap for keeping track of IDs
    bitmap: Bitmap,
    /// The range of the ID pool
    pool_range: Range<Id>,
}

// TODO: Use a ring ptr here?

impl IdTracker {
    /// Get an uninitilized instance of an `IdTracker`
    pub const fn uninit() -> Self {
        Self {
            bitmap: Bitmap::uninit(),
            pool_range: Id(0)..Id(0),
        }
    }

    /// Construct a new `IdTracker`
    pub fn new(pool_range: Range<Id>) -> Self {
        Self {
            bitmap: Bitmap::new(pool_range.end.0 - pool_range.start.0 + 1),
            pool_range,
        }
    }

    /// Try to find a free id in the pool_range and allocate it
    #[must_use = "Not freeing the ID will cause leaking"]
    pub fn allocate(&mut self) -> Result<Id, IdTrackerError> {
        let max_id = self.bitmap.used_bits_count();

        for i in 0..max_id {
            if !self.bitmap.is_set(i) {
                self.bitmap.set(i);
                return Ok(Id(i + self.pool_range.start.0));
            }
        }

        Err(IdTrackerError::OutOfIds)
    }

    pub fn allocate_at(&mut self, id: Id) -> Result<(), IdTrackerError> {
        if id.0 >= self.bitmap.used_bits_count() {
            return Err(IdTrackerError::InvalidId);
        } else if self.bitmap.is_set(id.0) {
            return Err(IdTrackerError::IdAlreadyTaken);
        }

        self.bitmap.set(id.0);

        Ok(())
    }

    // TODO: Give a handle or something to prevent bad freeing?
    /// Tries to free the given id
    pub unsafe fn free(&mut self, id: Id) -> Result<(), IdTrackerError> {
        // Make sure the ID is in the given pool_range
        if self.pool_range.end.0 < id.0 || id.0 < self.pool_range.start.0 {
            return Err(IdTrackerError::OutOfIds);
        }

        let index = id.0 - self.pool_range.start.0;

        // Free only if the ID is indeed already taken
        if self.bitmap.is_set(index) {
            self.bitmap.unset(index);

            return Ok(());
        }

        Err(IdTrackerError::IdAlreadyFree)
    }

    pub fn pool_range(&self) -> Range<Id> {
        self.pool_range.clone()
    }

    // pub fn grow_pool();
    // pub fn shrink_pool();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_allocator() {
        let mut allocator = IdTracker::new(Id(0)..Id(10));

        let id1 = allocator.allocate().unwrap();
        assert_eq!(id1.0, 0);

        let id2 = allocator.allocate().unwrap();
        assert_eq!(id2.0, 1);

        unsafe { allocator.free(id1).unwrap() };

        let id3 = allocator.allocate().unwrap();
        assert_eq!(id3.0, 0);
    }
}
