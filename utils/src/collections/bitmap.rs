use alloc::vec;
use alloc::vec::Vec;
use core::iter::Iterator;

/// A dynamic bitmap implementation with grow/shrink capabilities
#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    entries: Vec<u8>,
    used_bits_count: usize,
}

#[derive(Debug, PartialEq)]
pub enum BitmapError {
    IndexOutOfBounds { index: usize, size: usize },
    InvalidSize { size: usize },
}

impl Bitmap {
    /// Creates an uninitialized bitmap
    #[must_use]
    pub const fn uninit() -> Self {
        Self {
            entries: Vec::new(),
            used_bits_count: 0,
        }
    }

    /// Creates a new bitmap with specified number of bits
    #[must_use]
    pub fn new(used_bits_count: usize) -> Self {
        let entries_count = used_bits_count.div_ceil(8);
        Self {
            entries: vec![0; entries_count],
            used_bits_count,
        }
    }

    /// Sets the bit at the given index
    ///
    /// # Errors
    /// Returns an error if the index is out of bounds.
    pub fn set(&mut self, index: usize) -> Result<(), BitmapError> {
        if index >= self.used_bits_count {
            return Err(BitmapError::IndexOutOfBounds {
                index,
                size: self.used_bits_count,
            });
        }
        self.entries[index / 8] |= 1 << (index % 8);
        Ok(())
    }

    /// Unsets the bit at the given index
    ///
    /// # Errors
    /// Returns an error if the index is out of bounds.
    pub fn unset(&mut self, index: usize) -> Result<(), BitmapError> {
        if index >= self.used_bits_count {
            return Err(BitmapError::IndexOutOfBounds {
                index,
                size: self.used_bits_count,
            });
        }
        self.entries[index / 8] &= !(1 << (index % 8));
        Ok(())
    }

    /// Flips the bit at the given index
    ///
    /// # Errors
    /// Returns an error if the index is out of bounds.
    pub fn flip(&mut self, index: usize) -> Result<(), BitmapError> {
        if index >= self.used_bits_count {
            return Err(BitmapError::IndexOutOfBounds {
                index,
                size: self.used_bits_count,
            });
        }
        self.entries[index / 8] ^= 1 << (index % 8);
        Ok(())
    }

    /// Checks if the bit at index is set
    ///
    /// # Errors
    /// Returns an error if the index is out of bounds.
    pub fn is_set(&self, index: usize) -> Result<bool, BitmapError> {
        if index >= self.used_bits_count {
            return Err(BitmapError::IndexOutOfBounds {
                index,
                size: self.used_bits_count,
            });
        }
        Ok((self.entries[index / 8] & (1 << (index % 8))) != 0)
    }

    /// Returns the number of used bits
    #[must_use]
    pub const fn used_bits_count(&self) -> usize {
        self.used_bits_count
    }

    /// Grows the bitmap to accommodate `new_size` bits
    ///
    /// # Errors
    /// Returns an error if `new_size` is less than the current used bits count.
    pub fn grow(&mut self, new_size: usize) -> Result<(), BitmapError> {
        if new_size < self.used_bits_count {
            return Err(BitmapError::InvalidSize { size: new_size });
        }
        let new_entries_count = new_size.div_ceil(8);
        if new_entries_count > self.entries.len() {
            self.entries.resize(new_entries_count, 0);
        }
        self.used_bits_count = new_size;
        Ok(())
    }

    /// Shrinks the bitmap to `new_size` bits
    ///
    /// # Errors
    /// Returns an error if `new_size` is greater than the current used bits count.
    pub fn shrink(&mut self, new_size: usize) -> Result<(), BitmapError> {
        if new_size > self.used_bits_count {
            return Err(BitmapError::InvalidSize { size: new_size });
        }
        let new_entries_count = new_size.div_ceil(8);
        self.entries.truncate(new_entries_count);
        self.used_bits_count = new_size;
        Ok(())
    }

    /// Clears all bits in the bitmap
    pub fn clear(&mut self) {
        self.entries.iter_mut().for_each(|entry| *entry = 0);
    }

    /// Creates an iterator over the bitmap bits
    #[must_use]
    pub fn iter(&self) -> BitmapIterator<'_> {
        BitmapIterator {
            bitmap: self,
            current_index: 0,
        }
    }
}

/// Iterator over bitmap bits
pub struct BitmapIterator<'a> {
    bitmap: &'a Bitmap,
    current_index: usize,
}

impl Iterator for BitmapIterator<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.bitmap.used_bits_count {
            return None;
        }
        let result = self.bitmap.is_set(self.current_index).unwrap_or(false);
        self.current_index += 1;
        Some(result)
    }
}

impl IntoIterator for Bitmap {
    type Item = bool;
    type IntoIter = BitmapIntoIterator;

    fn into_iter(self) -> Self::IntoIter {
        BitmapIntoIterator {
            bitmap: self,
            current_index: 0,
        }
    }
}

/// IntoIterator implementation for owned bitmap
pub struct BitmapIntoIterator {
    bitmap: Bitmap,
    current_index: usize,
}

impl Iterator for BitmapIntoIterator {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.bitmap.used_bits_count {
            return None;
        }
        let result = self.bitmap.is_set(self.current_index).unwrap_or(false);
        self.current_index += 1;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_bitmap() {
        let bitmap = Bitmap::new(10);
        assert_eq!(bitmap.used_bits_count(), 10);
        assert_eq!(bitmap.entries.len(), 2);
        assert!(bitmap.iter().all(|bit| !bit));
    }

    #[test]
    fn test_set_unset() {
        let mut bitmap = Bitmap::new(8);
        assert!(bitmap.set(2).is_ok());
        assert!(bitmap.is_set(2).unwrap());
        assert!(bitmap.unset(2).is_ok());
        assert!(!bitmap.is_set(2).unwrap());
    }

    #[test]
    fn test_out_of_bounds() {
        let mut bitmap = Bitmap::new(8);
        assert!(matches!(
            bitmap.set(8),
            Err(BitmapError::IndexOutOfBounds { index: 8, size: 8 })
        ));
        assert!(matches!(
            bitmap.is_set(8),
            Err(BitmapError::IndexOutOfBounds { index: 8, size: 8 })
        ));
    }

    #[test]
    fn test_grow_shrink() {
        let mut bitmap = Bitmap::new(8);
        assert!(bitmap.grow(16).is_ok());
        assert_eq!(bitmap.used_bits_count(), 16);
        assert!(bitmap.set(15).is_ok());

        assert!(bitmap.shrink(10).is_ok());
        assert_eq!(bitmap.used_bits_count(), 10);
        assert!(matches!(
            bitmap.set(15),
            Err(BitmapError::IndexOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_iterator() {
        let mut bitmap = Bitmap::new(8);
        bitmap.set(1).unwrap();
        bitmap.set(3).unwrap();
        let bits: Vec<bool> = bitmap.iter().collect();
        assert_eq!(
            bits,
            vec![false, true, false, true, false, false, false, false]
        );
    }

    #[test]
    fn test_into_iterator() {
        let mut bitmap = Bitmap::new(4);
        bitmap.set(0).unwrap();
        bitmap.set(2).unwrap();
        let bits: Vec<bool> = bitmap.into_iter().collect();
        assert_eq!(bits, vec![true, false, true, false]);
    }

    #[test]
    fn test_invalid_resize() {
        let mut bitmap = Bitmap::new(8);
        assert!(matches!(
            bitmap.grow(7),
            Err(BitmapError::InvalidSize { size: 7 })
        ));
        assert!(matches!(
            bitmap.shrink(9),
            Err(BitmapError::InvalidSize { size: 9 })
        ));
    }
}
