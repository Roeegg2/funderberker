use utils::{
    collections::id::{Id, hander::IdHander},
    mem::{HHDM_OFFSET, VirtAddr},
    sanity_assert,
    sync::spinlock::{SpinLock, SpinLockable},
};

use crate::arch::BASIC_PAGE_SIZE;

pub(crate) static VAA: SpinLock<VirtualAddressAllocator> =
    SpinLock::new(VirtualAddressAllocator::uninit());

pub struct VirtualAddressAllocator {
    hander: IdHander,
}

impl VirtualAddressAllocator {
    pub fn new(start_addr: VirtAddr) -> Self {
        // The minimal memory range we demand
        const MIN_MEM_SPAN: usize = 8 * 0x1000 * 0x1000 * 0x1000 * 0x1000; // 8TB

        // Making sure address is page aligned
        sanity_assert!(start_addr.0 % BASIC_PAGE_SIZE == 0);

        // Make sure we have enough virtual memory space
        assert!(
            HHDM_OFFSET.get() - start_addr.0 >= MIN_MEM_SPAN,
            "Cannot find enough virtual memory space"
        );

        logger::info!("VAA initialized with start address of {:?}", start_addr);

        let start_id = Id(start_addr.0 / BASIC_PAGE_SIZE);
        Self {
            hander: IdHander::new_starting_from(start_id, Id::MAX_ID),
        }
    }

    #[inline]
    const fn uninit() -> Self {
        Self {
            hander: IdHander::new(Id(1000)),
        }
    }

    #[inline]
    pub(super) fn handout(&mut self, count: usize, page_alignment: usize) -> VirtAddr {
        let next = self.hander.peek_next().0;
        let skip = (next as *const ()).align_offset(page_alignment);

        let page_id = self
            .hander
            .handout_and_skip(skip + count)
            .expect("Virtual address allocator ran out of IDs");

        VirtAddr(page_id.0 * BASIC_PAGE_SIZE)
    }
}

impl SpinLockable for VirtualAddressAllocator {}
