//! A simple bitmap implementation, with the option to grow/shrink the bitmap

#[cfg(test)]
use std::vec;
#[cfg(test)]
use std::vec::Vec;
#[cfg(not(test))]
use alloc::vec;
#[cfg(not(test))]
use alloc::vec::Vec;


/// The bitmap 
pub struct Bitmap {
    entries: Vec<u8>,
    used_bits_count: usize,
}

impl Bitmap {
    /// Get an uninitilized instance of a bitmap
    pub const fn uninit() -> Self {
        Self {
            entries: Vec::new(),
            used_bits_count: 0,
        }
    }

    /// Construct a new bitmap
    pub fn new(used_bits_count: usize) -> Self {
        let entries_count = (used_bits_count + 7) / 8;
        Self {
            entries: vec![0; entries_count],
            used_bits_count,
        }
    }

    /// Unset the bit with the given `index`
    pub fn unset(&mut self, index: usize) {
        self.entries[index / 8] &= !(1 << (index % 8));
    }

    /// Set the bit with the given `index`
    pub fn set(&mut self, index: usize) {
        self.entries[index / 8] |= 1 << (index % 8);
    }

    /// Check if the bit at `index` is set. `true` if set, `false` otherwise
    pub fn is_set(&self, index: usize) -> bool {
        let entry = self.entries[index / 8];

        (entry & (1 << (index % 8))) != 0
    }

    /// Get `used_bits_count` readonly value
    pub const fn used_bits_count(&self) -> usize {
        self.used_bits_count
    }
}

