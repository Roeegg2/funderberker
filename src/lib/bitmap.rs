//! Bitmap data structure and wrappers
// TODO: Add memsetting, iter, etc

pub struct Bitmap<'a> {
    entries: &'a mut [u8],
    used_bits_count: usize,
}

impl<'a> Bitmap<'a> {
    pub const BLOCK_TAKEN: u8 = 0xff;
    pub const FREE: u8 = 0x0;

    /// Get an uninitilized instance of a bitmap
    pub const fn uninit() -> Self {
        Self {
            entries: &mut [],
            used_bits_count: 0,
        }
    }

    /// Construct a new bitmap
    pub const fn new(entries: &'a mut [u8], used_bits_count: usize) -> Self {
        Self {
            entries,
            used_bits_count,
        }
    }

    /// Index into the bitmap and unset the status of an entry
    pub const fn unset(&mut self, index: usize) {
        self.entries[index / 8] &= !(1 << (index % 8));
    }

    /// Index into the bitmap and set the status of a page
    pub const fn set(&mut self, index: usize) {
        self.entries[index / 8] |= 1 << (index % 8);
    }

    /// Index into the bitmap and get the status of a page
    pub const fn get(&self, index: usize) -> u8 {
        self.entries[index / 8] & (1 << (index % 8))
    }

    // NOTE: not sure having this function is the correct way to provide a readonly view into
    // `used_bits_count` in Rust...
    /// Get `used_bits_count` readonly value
    pub const fn used_bits_count(&self) -> usize {
        self.used_bits_count
    }
}

// impl
// fn set
// fn unset
// fn get
// fn new
