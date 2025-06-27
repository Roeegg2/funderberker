use core::{marker::PhantomData, num::NonZero, ptr::NonNull};
use pmm::PmmAllocator;
use utils::mem::{PhysAddr, VirtAddr};

use super::vaa::VAA;

pub struct Flags<P>
where
    P: PagingManager,
{
    data: usize,
    _arch: PhantomData<P>,
}

pub struct PageSize<P>
where
    P: PagingManager,
{
    size: usize,
    _arch: PhantomData<P>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingError {
    PageAlreadyPresent,
    PageNotPresent,
    PageFault,
    InvalidPageSize,
    InvalidAddress,
    InvalidFlags,
    OutOfMemory,
    BadPageCountAndAddressCombination,
}

pub trait PagingManager: Sized {
    const BASIC_PAGE_SIZE: PageSize<Self>;

    unsafe fn map_pages_to(
        phys_addr: PhysAddr,
        virt_addr: VirtAddr,
        count: usize,
        flags: Flags<Self>,
        page_size: PageSize<Self>,
    ) -> Result<(), PagingError>;

    unsafe fn unmap_pages(
        virt_addr: VirtAddr,
        page_count: usize,
        age_size: PageSize<Self>,
    ) -> Result<(), PagingError>;

    fn translate(virt_addr: VirtAddr) -> Option<PhysAddr>;

    fn allocate_pages(
        page_count: usize,
        flags: Flags<Self>,
        page_size: PageSize<Self>,
    ) -> Result<NonNull<()>, PagingError> {
        let base_virt_addr = {
            let mut vaa = VAA.lock();
            let page_id = vaa.handout(page_count, page_size.page_alignment());
            VirtAddr(page_id.0 * page_size.size())
        };

        let basic_page_count = page_size.to_default_page_count();
        // TODO: Do this in bulk
        for i in 0..page_count {
            let virt_addr = base_virt_addr + (i * page_size.size());
            let phys_addr = pmm::get().allocate(1, basic_page_count).unwrap();

            unsafe {
                Self::map_pages_to(phys_addr, virt_addr, 1, flags, page_size)?;
            }
        }


        Ok(NonNull::without_provenance(
            NonZero::new(base_virt_addr.0).unwrap(),
        ))
    }

    unsafe fn free_pages(
        ptr: NonNull<()>,
        count: usize,
        page_size: PageSize<Self>,
    ) -> Result<(), PagingError> {
        let virt_addr = ptr.into();
        for i in 0..count {
            let addr = virt_addr + (i * page_size.size());
            let phys_addr = Self::translate(virt_addr).ok_or(PagingError::PageNotPresent)?;
            // XXX: need to make sure we uunmap and then free
            unsafe {
                Self::unmap_pages(addr, count, page_size)?;

                pmm::get()
                    .free(phys_addr, page_size.to_default_page_count())
                    .map_err(|_| PagingError::PageNotPresent)?;
            };
        }

        Ok(())
    }

    unsafe fn map_pages(
        phys_addr: PhysAddr,
        page_count: usize,
        flags: Flags<Self>,
        page_size: PageSize<Self>,
    ) -> Result<*mut (), PagingError> {

        let virt_addr = {
            let mut vaa = VAA.lock();
            vaa.handout(1, page_size.page_alignment())
        };

        unsafe {
            Self::map_pages_to(phys_addr, virt_addr, page_count, flags, page_size)?;
        };

        Ok(core::ptr::without_provenance_mut(virt_addr.0))
    }

    #[cfg(feature = "limine")]
    unsafe fn init_paging_from_limine(
        mem_map: &[&limine::memory_map::Entry],
        kernel_virt: VirtAddr,
        kernel_phys: PhysAddr,
        used_by_pmm: &limine::memory_map::Entry,
    );
}

#[inline]
pub fn allocate_pages<P>(
    count: usize,
    flags: Flags<P>,
    page_size: PageSize<P>,
) -> Result<NonNull<()>, PagingError>
where
    P: PagingManager,
{
    P::allocate_pages(count, flags, page_size)
}

#[inline]
pub unsafe fn free_pages<P>(
    ptr: NonNull<()>,
    count: usize,
    page_size: PageSize<P>,
) -> Result<(), PagingError>
where
    P: PagingManager,
{
    unsafe { P::free_pages(ptr, count, page_size) }
}

impl<P> PageSize<P>
where
    P: PagingManager,
{
    #[inline]
    pub const fn size(self) -> usize {
        self.size
    }

    #[inline]
    pub const fn page_alignment(self) -> usize {
        self.size / P::BASIC_PAGE_SIZE.size
    }

    #[inline]
    pub const fn to_default_page_count(self) -> usize {
        self.size / P::BASIC_PAGE_SIZE.size
    }

    #[inline]
    pub(crate) const unsafe fn from_raw(size: usize) -> Self {
        Self {
            size,
            _arch: PhantomData,
        }
    }
}

impl<P> Flags<P>
where
    P: PagingManager,
{
    #[inline]
    #[must_use]
    pub const fn data(self) -> usize {
        self.data
    }

    #[inline]
    #[must_use]
    pub(crate) const fn get(self, data: usize) -> bool {
        (self.data & data) != 0
    }

    #[inline]
    #[must_use]
    pub(crate) const fn set(mut self, data: usize, status: bool) -> Self {
        if status {
            self.data |= data;
        } else {
            self.data &= !data;
        }

        self
    }

    #[inline]
    #[must_use]
    pub(crate) const unsafe fn from_raw(data: usize) -> Self {
        Self {
            data,
            _arch: PhantomData,
        }
    }

    pub const unsafe fn join(self, other: Flags<P>) -> Option<Self> {
        if self.data & other.data != 0 {
            return None; // Overlapping flags
        }

        Some(self.set(other.data, true))
    }
}

impl<P> Clone for Flags<P>
where
    P: PagingManager,
{
    fn clone(&self) -> Self {
        Flags {
            data: self.data,
            _arch: PhantomData,
        }
    }
}

impl<P> Copy for Flags<P> where P: PagingManager {}

impl<P> Clone for PageSize<P>
where
    P: PagingManager,
{
    fn clone(&self) -> Self {
        PageSize {
            size: self.size,
            _arch: PhantomData,
        }
    }
}

impl<P> Copy for PageSize<P> where P: PagingManager {}
