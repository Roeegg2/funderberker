use crate::{collections::bitmap::Bitmap, sync::spinlock::SpinLockable};

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
    /// The minimum ID that can be allocated
    min: Id,
}

// TODO: Use a ring ptr here?

impl IdTracker {
    // TODO: Remove this and use the `Default` when const default is possible
    /// Get an uninitilized instance of an `IdTracker`
    #[must_use]
    pub const fn uninit() -> Self {
        Self {
            bitmap: Bitmap::uninit(),
            min: Id(0),
        }
    }

    /// Construct a new `IdTracker`
    #[must_use]
    pub fn new(min: Id, max: Id) -> Self {
        Self {
            bitmap: Bitmap::new(max.0 - min.0 + 1),
            min,
        }
    }

    /// Try to find a free id in the `pool_range` and allocate it
    #[must_use = "Not freeing the ID will cause leaking"]
    pub fn allocate(&mut self) -> Result<Id, IdTrackerError> {
        for i in 0..self.bitmap.used_bits_count() {
            if !self
                .bitmap
                .is_set(i)
                .map_err(|_| IdTrackerError::InvalidId)?
            {
                let id = Id(self.min.0 + i);
                self.bitmap.set(i).unwrap();
                return Ok(id);
            }
        }

        Err(IdTrackerError::OutOfIds)
    }

    pub fn allocate_at(&mut self, id: Id) -> Result<(), IdTrackerError> {
        if id < self.min || id > Id(self.min.0 + self.bitmap.used_bits_count() - 1) {
            return Err(IdTrackerError::InvalidId);
        }
        let bit_index = id.0 - self.min.0;
        let found = self.bitmap.is_set(bit_index).unwrap();
        if found {
            return Err(IdTrackerError::IdAlreadyTaken);
        }

        self.bitmap.set(bit_index).unwrap();

        Ok(())
    }

    // TODO: Give a handle or something to prevent bad freeing?
    /// Tries to free the given id
    pub unsafe fn free(&mut self, id: Id) -> Result<(), IdTrackerError> {
        if id < self.min || id > Id(self.min.0 + self.bitmap.used_bits_count() - 1) {
            return Err(IdTrackerError::InvalidId);
        }
        let bit_index = id.0 - self.min.0;
        let found = self.bitmap.is_set(bit_index).unwrap();
        if !found {
            return Err(IdTrackerError::IdAlreadyFree);
        }

        self.bitmap.unset(bit_index).unwrap();

        Ok(())
    }
}

impl SpinLockable for IdTracker {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_single_id() {
        let mut tracker = IdTracker::new(Id(0), Id(10));
        let allocated = tracker.allocate().unwrap();
        assert_eq!(allocated, Id(0));
    }

    #[test]
    fn test_allocate_multiple_ids() {
        let mut tracker = IdTracker::new(Id(0), Id(5));

        let id1 = tracker.allocate().unwrap();
        let id2 = tracker.allocate().unwrap();
        let id3 = tracker.allocate().unwrap();

        assert_eq!(id1, Id(0));
        assert_eq!(id2, Id(1));
        assert_eq!(id3, Id(2));
    }

    #[test]
    fn test_allocate_with_offset_range() {
        let mut tracker = IdTracker::new(Id(10), Id(15));
        let allocated = tracker.allocate().unwrap();
        assert_eq!(allocated, Id(10));
    }

    #[test]
    #[should_panic]
    fn test_allocate_exhaustion() {
        let mut tracker = IdTracker::new(Id(0), Id(2));

        // Allocate all available IDs
        tracker.allocate().unwrap();
        tracker.allocate().unwrap();
        tracker.allocate().unwrap();

        // Should panic on the fourth allocation
        tracker.allocate().unwrap();
    }

    #[test]
    fn test_allocate_at_success() {
        let mut tracker = IdTracker::new(Id(0), Id(10));
        let result = tracker.allocate_at(Id(5));
        assert!(result.is_ok());
    }

    #[test]
    fn test_allocate_at_invalid_id() {
        let mut tracker = IdTracker::new(Id(0), Id(5));
        let result = tracker.allocate_at(Id(10));
        assert_eq!(result, Err(IdTrackerError::InvalidId));
    }

    #[test]
    fn test_allocate_at_already_taken() {
        let mut tracker = IdTracker::new(Id(0), Id(10));

        // First allocation should succeed
        tracker.allocate_at(Id(3)).unwrap();

        // Second allocation of same ID should fail
        let result = tracker.allocate_at(Id(3));
        assert_eq!(result, Err(IdTrackerError::IdAlreadyTaken));
    }

    #[test]
    fn test_free_allocated_id() {
        let mut tracker = IdTracker::new(Id(0), Id(10));
        let allocated = tracker.allocate().unwrap();

        let result = unsafe { tracker.free(allocated) };
        assert!(result.is_ok());
    }

    #[test]
    fn test_free_id_out_of_range_high() {
        let mut tracker = IdTracker::new(Id(0), Id(5));
        let result = unsafe { tracker.free(Id(10)) };
        assert_eq!(result, Err(IdTrackerError::InvalidId));
    }

    #[test]
    fn test_free_id_out_of_range_low() {
        let mut tracker = IdTracker::new(Id(5), Id(10));
        let result = unsafe { tracker.free(Id(2)) };
        assert_eq!(result, Err(IdTrackerError::InvalidId));
    }

    #[test]
    fn test_free_already_free_id() {
        let mut tracker = IdTracker::new(Id(0), Id(10));
        let allocated = tracker.allocate().unwrap();

        // Free once - should succeed
        unsafe { tracker.free(allocated) }.unwrap();

        // Free again - should fail
        let result = unsafe { tracker.free(allocated) };
        assert_eq!(result, Err(IdTrackerError::IdAlreadyFree));
    }

    #[test]
    fn test_free_never_allocated_id() {
        let mut tracker = IdTracker::new(Id(0), Id(10));
        let result = unsafe { tracker.free(Id(5)) };
        assert_eq!(result, Err(IdTrackerError::IdAlreadyFree));
    }

    #[test]
    fn test_allocate_after_free() {
        let mut tracker = IdTracker::new(Id(0), Id(3));

        // Allocate all IDs
        let id1 = tracker.allocate().unwrap();
        let id2 = tracker.allocate().unwrap();
        let id3 = tracker.allocate().unwrap();
        let id4 = tracker.allocate().unwrap();

        assert_eq!(id1, Id(0));
        assert_eq!(id2, Id(1));
        assert_eq!(id3, Id(2));
        assert_eq!(id4, Id(3));

        // Should be out of IDs
        assert_eq!(tracker.allocate(), Err(IdTrackerError::OutOfIds));

        // Free the middle ID
        unsafe { tracker.free(id2) }.unwrap();

        // Should be able to allocate again and get the freed ID
        let reused_id = tracker.allocate().unwrap();
        assert_eq!(reused_id, Id(1));
    }

    #[test]
    fn test_complex_allocation_pattern() {
        let mut tracker = IdTracker::new(Id(10), Id(15));

        // Allocate some IDs
        let id1 = tracker.allocate().unwrap();
        let id2 = tracker.allocate().unwrap();
        let id3 = tracker.allocate().unwrap();

        assert_eq!(id1, Id(10));
        assert_eq!(id2, Id(11));
        assert_eq!(id3, Id(12));

        // Free the first one
        unsafe { tracker.free(id1) }.unwrap();

        // Allocate again - should reuse the freed ID
        let reused = tracker.allocate().unwrap();
        assert_eq!(reused, Id(10));

        // Continue allocating
        let id4 = tracker.allocate().unwrap();
        let id5 = tracker.allocate().unwrap();
        let id6 = tracker.allocate().unwrap();

        assert_eq!(id4, Id(13));
        assert_eq!(id5, Id(14));
        assert_eq!(id6, Id(15));

        // Should be exhausted now
        assert_eq!(tracker.allocate(), Err(IdTrackerError::OutOfIds));
    }

    #[test]
    fn test_edge_case_single_id_pool() {
        let mut tracker = IdTracker::new(Id(5), Id(5));
        let allocated = tracker.allocate().unwrap();
        assert_eq!(allocated, Id(5));

        // Should be exhausted
        assert_eq!(tracker.allocate(), Err(IdTrackerError::OutOfIds));

        // Free and reallocate
        unsafe { tracker.free(allocated) }.unwrap();
        let reallocated = tracker.allocate().unwrap();
        assert_eq!(reallocated, Id(5));
    }
}
