use core::fmt::Debug;

use crate::{
    arch::x86_64::X86_64,
    mem::paging::{Flags, PageSize},
};

pub(super) const MAX_BOTTOM_PAGING_LEVEL: usize = 3;

impl PageSize<X86_64> {
    const SIZE_4KB: usize = 0x1000; // 4KB page size
    const SIZE_2MB: usize = 0x200000; // 2MB page size
    const SIZE_1GB: usize = 0x40000000; // 1GB page size

    #[inline]
    #[must_use]
    pub const fn size_4kb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_4KB) }
    }

    #[inline]
    #[must_use]
    pub const fn size_2mb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_2MB) }
    }

    #[inline]
    #[must_use]
    pub const fn size_1gb() -> Self {
        unsafe { Self::from_raw(Self::SIZE_1GB) }
    }

    #[inline]
    #[must_use]
    pub(super) const fn from_bottom_paging_level(level: usize) -> Option<Self> {
        match level {
            0 => Some(Self::size_4kb()),
            1 => Some(Self::size_2mb()),
            2 => Some(Self::size_1gb()),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub(super) const fn bottom_paging_level(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 0, // PTE
            Self::SIZE_2MB => 1, // PDE
            Self::SIZE_1GB => 2, // PDPE
            _ => unreachable!(),
        }
    }

    #[inline]
    #[must_use]
    pub(super) const fn offset_bit_count(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 12, // 2^12 = 4096
            Self::SIZE_2MB => 21, // 2^21 = 2097152
            Self::SIZE_1GB => 30, // 2^30 = 1073741824
            _ => unreachable!(),
        }
    }

    #[inline]
    #[must_use]
    pub(super) const fn get_offset_mask(self) -> usize {
        self.size() - 1
    }

    #[inline]
    #[must_use]
    pub(super) const fn get_pat_bit(self) -> usize {
        match self.size() {
            Self::SIZE_4KB => 7,
            Self::SIZE_2MB | Self::SIZE_1GB => 12,
            _ => unreachable!(),
        }
    }
}

impl Into<Flags<X86_64>> for PageSize<X86_64> {
    fn into(self) -> Flags<X86_64> {
        match self.size() {
            Self::SIZE_4KB => Flags::<X86_64>::new(),
            _ => Flags::<X86_64>::new().set_page_size(true),
        }
    }
}

impl PartialEq for PageSize<X86_64> {
    fn eq(&self, other: &Self) -> bool {
        self.size() == other.size()
    }
}

impl Debug for PageSize<X86_64> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.size() {
            Self::SIZE_4KB => write!(f, "PageSize::Size4KB"),
            Self::SIZE_2MB => write!(f, "PageSize::Size2MB"),
            Self::SIZE_1GB => write!(f, "PageSize::Size1GB"),
            _ => write!(f, "PageSize::Unknown({})", self.size()),
        }
    }
}
