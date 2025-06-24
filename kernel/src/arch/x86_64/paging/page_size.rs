use crate::{
    arch::x86_64::X86_64,
    mem::paging::{Flags, PageSize},
};

pub(super) const MAX_BOTTOM_PAGING_LEVEL: usize = 3;

impl PageSize<X86_64> {
    pub(super) const SIZE_4KB: usize = 0x1000; // 4KB page size
    pub(super) const SIZE_2MB: usize = 0x200000; // 2MB page size
    pub(super) const SIZE_1GB: usize = 0x40000000; // 1GB page size

    #[inline]
    pub const fn size_4kb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_4KB) }
    }

    #[inline]
    pub const fn size_2mb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_2MB) }
    }

    #[inline]
    pub const fn size_1gb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_1GB) }
    }

    #[inline]
    pub(super) const fn flag(self) -> Flags<X86_64> {
        match self.size() {
            Self::SIZE_4KB => Flags::<X86_64>::new(),
            _ => Flags::<X86_64>::new().set_page_size(true),
        }
    }

    #[inline]
    pub(super) const fn offset_bit_count(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 12, // 2^12 = 4096
            Self::SIZE_2MB => 21, // 2^21 = 2097152
            Self::SIZE_1GB => 30, // 2^30 = 1073741824
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(super) const fn bottom_paging_level(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 0, // PTE
            Self::SIZE_2MB => 1, // PDE
            Self::SIZE_1GB => 2, // PDPE
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(super) const fn get_pat_bit(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 7,
            Self::SIZE_2MB | Self::SIZE_1GB => 12,
            _ => unreachable!(),
        }
    }
}

impl PartialEq for PageSize<X86_64> {
    fn eq(&self, other: &Self) -> bool {
        self.size() == other.size()
    }
}
