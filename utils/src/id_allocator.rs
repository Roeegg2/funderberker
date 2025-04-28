use core::ops::Range;

use crate::{collections::bitmap::Bitmap, sanity_assert};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IdAllocatorError {
    OutOfIds,
    IdAlreadyFree,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Id(pub usize);

pub struct IdAllocator {
    bitmap: Bitmap,
    range: Range<Id>,
}

// TODO: Use a ring ptr here?

impl IdAllocator {
    /// Get an uninitilized instance of an `IdAllocator`
    pub const fn uninit() -> Self {
        Self {
            bitmap: Bitmap::uninit(),
            range: Id(0)..Id(0),
        }
    }

    /// Construct a new `IdAllocator`
    pub fn new(range: Range<Id>) -> Self {
        Self {
            bitmap: Bitmap::new(range.end.0 - range.start.0 + 1),
            range,
        }
    }

    /// Try to find a free id in the range and allocate it
    #[must_use = "Not freeing the ID will cause leaking"]
    pub fn allocate(&mut self) -> Result<Id, IdAllocatorError> {
        let max_id = self.bitmap.used_bits_count();

        for i in 0..max_id {
            if !self.bitmap.is_set(i) {
                self.bitmap.set(i);
                return Ok(Id(i + self.range.start.0));
            }
        }

        Err(IdAllocatorError::OutOfIds)
    }

    // TODO: Give a handle or something to prevent bad freeing?
    /// Tries to free the given id
    pub unsafe fn free(&mut self, id: Id) -> Result<(), IdAllocatorError> {
        sanity_assert!(id.0 >= self.range.start.0 && id.0 <= self.range.end.0);

        let index = id.0 - self.range.start.0;
        if self.bitmap.is_set(index) {
            self.bitmap.unset(index);

            return Ok(());
        }

        Err(IdAllocatorError::IdAlreadyFree)
    }

    // pub fn grow_pool();
    // pub fn shrink_pool();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_allocator() {
        let mut allocator = IdAllocator::new(Id(0)..Id(10));

        let id1 = allocator.allocate().unwrap();
        assert_eq!(id1.0, 0);

        let id2 = allocator.allocate().unwrap();
        assert_eq!(id2.0, 1);

        unsafe {allocator.free(id1).unwrap()};

        let id3 = allocator.allocate().unwrap();
        assert_eq!(id3.0, 0);
    }
}
