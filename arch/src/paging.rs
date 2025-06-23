use core::marker::PhantomData;

use crate::Arch;

pub struct Flags<A>
where
    A: Arch,
{
    pub(super) data: usize,
    pub(super) _arch: PhantomData<A>,
}

pub struct PageSize<A>
where
    A: Arch,
{
    pub(super) size: usize,
    pub(super) _arch: PhantomData<A>,
}

#[derive(Debug, Clone, Copy)]
pub enum PagingError {
    PageNotPresent,
    PageFault,
    InvalidPageSize,
    InvalidAddress,
}

impl<A> PageSize<A>
where
    A: Arch,
{
    #[inline]
    pub const fn size(self) -> usize {
        self.size
    }

    #[inline]
    pub const fn alignment(self) -> usize {
        self.size / A::BASIC_PAGE_SIZE.size
    }

    #[inline]
    pub const fn to_default_page_count(self) -> usize {
        self.size / A::BASIC_PAGE_SIZE.size
    }
}

impl<A> Clone for Flags<A>
where
    A: Arch,
{
    fn clone(&self) -> Self {
        Flags {
            data: self.data,
            _arch: PhantomData,
        }
    }
}

impl<A> Copy for Flags<A> where A: Arch {}

impl<A> Clone for PageSize<A>
where
    A: Arch,
{
    fn clone(&self) -> Self {
        PageSize {
            size: self.size,
            _arch: PhantomData,
        }
    }
}

impl<A> Copy for PageSize<A> where A: Arch {}
