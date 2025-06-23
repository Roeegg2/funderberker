use core::marker::PhantomData;

use crate::{paging::{Flags, PageSize}, x86_64::X86_64};

#[cfg(feature = "paging_4")]
pub(super) const MAX_BOTTOM_PAGING_LEVEL: usize = 3;
#[cfg(feature = "paging_5")]
pub(super) const MAX_BOTTOM_PAGING_LEVEL: usize = 4;

impl PageSize<X86_64> {
    pub(super) const SIZE_4KB: usize = 0x1000; // 4KB page size
    pub(super) const SIZE_2MB: usize = 0x200000; // 2MB page size
    pub(super) const SIZE_1GB: usize = 0x40000000; // 1GB page size

    #[inline]
    pub const fn size_4kb() -> Self {
        Self {
            size: Self::SIZE_4KB,
            _arch: PhantomData,
        }
    }

    #[inline]
    pub const fn size_2mb() -> Self {
        Self {
            size: Self::SIZE_2MB,
            _arch: PhantomData,
        }
    }

    #[inline]
    pub const fn size_1gb() -> Self {
        Self {
            size: Self::SIZE_1GB,
            _arch: PhantomData,
        }
    }

    #[inline]
    pub(super) const fn flag(self) -> Flags<X86_64> {
        match self.size {
            Self::SIZE_4KB => Flags::<X86_64>::new(),
            _ => Flags::<X86_64>::new().set_page_size(true),
        }
    }

    #[inline]
    pub(super) const fn offset_bit_count(self) -> usize {
        match self.size {
            Self::SIZE_4KB => 12, // 2^12 = 4096
            Self::SIZE_2MB => 21, // 2^21 = 2097152
            Self::SIZE_1GB => 30, // 2^30 = 1073741824
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(super) const fn bottom_paging_level(self) -> usize {
        match self.size {
            Self::SIZE_4KB => 0, // PTE
            Self::SIZE_2MB => 1, // PDE
            Self::SIZE_1GB => 2, // PDPE
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(super) const fn get_pat_bit(self) -> usize {
        match self.size {
            Self::SIZE_4KB => 7,
            Self::SIZE_2MB | Self::SIZE_1GB => 12,
            _ => unreachable!(),
        }
    }
}
