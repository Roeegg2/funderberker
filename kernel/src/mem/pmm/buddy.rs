//! A buddy allocator for the PMM

use core::{num::NonZero, ptr::NonNull, slice::from_raw_parts_mut};

use alloc::boxed::Box;
use limine::memory_map::EntryType;
use utils::collections::stacklist::{Node, StackList};

use crate::{arch::BASIC_PAGE_SIZE, boot::limine::get_page_count_from_mem_map, mem::PhysAddr};

use super::{PmmAllocator, PmmError};

/// Singleton instance of the buddy allocator
pub(super) static mut BUDDY_ALLOCATOR: BuddyAllocator = BuddyAllocator {
    zones: &mut [],
    freelist: StackList::new(),
    freelist_refill_zone_index: 0,
};

/// A buddy allocator for the PMM
#[derive(Debug)]
pub(super) struct BuddyAllocator<'a> {
    zones: &'a mut [StackList<PhysAddr>],
    freelist: StackList<PhysAddr>,
    freelist_refill_zone_index: usize,
}

impl<'a> PmmAllocator for BuddyAllocator<'a> {
    fn alloc_at(&mut self, addr: PhysAddr, mut page_count: NonZero<usize>) -> Result<(), PmmError> {
        if !self.is_page_free(addr, page_count)? {
            return Err(PmmError::NoAvailableBlock);
        }

        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        let zone_index = Self::page_count_to_index(page_count);
        Self::check_address_alignment(addr, zone_index)?;

        let bucket_zone_index = self.find_bucket_at(addr, zone_index)?;
        self.disband(addr, bucket_zone_index, zone_index);

        Ok(())
    }

    fn alloc_any(
        &mut self,
        alignment: NonZero<usize>,
        mut page_count: NonZero<usize>,
    ) -> Result<PhysAddr, PmmError> {
        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        let zone_index = Self::page_count_to_index(page_count);

        let (addr, bucket_index) = self.find_bucket_any(alignment, zone_index)?;
        //println!("addr: {:#x}, bucket_index: {:#x} zone_index {:#x}", addr.0, bucket_index, zone_index);
        self.disband(addr, bucket_index, zone_index);

        Ok(addr)
    }

    fn is_page_free(
        &self,
        addr: PhysAddr,
        mut page_count: NonZero<usize>,
    ) -> Result<bool, PmmError> {
        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        let zone_index = Self::page_count_to_index(page_count);
        Self::check_address_alignment(addr, zone_index)?;

        for i in zone_index..self.zones.len() {
            for bucket in self.zones[i].iter() {
                if *bucket == addr {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    // TODO: Maybe do it the old way, by breaking it down into blocks and then coalescing instead
    // of just everything as page sizes?
    unsafe fn free(
        &mut self,
        addr: PhysAddr,
        mut page_count: NonZero<usize>,
    ) -> Result<(), PmmError> {
        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;
        if self.is_page_free(addr, page_count)? {
            return Err(PmmError::FreeOfAlreadyFree);
        }

        let zone_index = Self::page_count_to_index(page_count);

        self.coalesce(addr, zone_index);

        Ok(())
    }

    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        let total_page_count = get_page_count_from_mem_map(mem_map);

        #[allow(static_mut_refs)]
        let (freelist_entry_addr, entries_page_count) =
            unsafe { BUDDY_ALLOCATOR.init_freelist(mem_map, total_page_count) };

        for entry in mem_map.iter() {
            match entry.entry_type {
                EntryType::USABLE => {
                    let page_count = entry.length as usize / BASIC_PAGE_SIZE;
                    let addr = PhysAddr(entry.base as usize);
                    unsafe {
                        #[allow(static_mut_refs)]
                        BUDDY_ALLOCATOR
                            .break_into_buckets_n_free(addr, NonZero::new(page_count).unwrap());
                    };
                }
                _ => continue,
            }
        }

        for i in 0..entries_page_count.get() {
            unsafe {
                #[allow(static_mut_refs)]
                BUDDY_ALLOCATOR
                    .alloc_at(
                        PhysAddr(freelist_entry_addr.0 + (i * BASIC_PAGE_SIZE)),
                        NonZero::new_unchecked(1),
                    )
                    .unwrap();
            }
        }
    }
}

impl<'a> BuddyAllocator<'a> {
    /// The lowest possible zone level (the zone level of `BASIC_PAGE_SIZE`)
    const MIN_ZONE_LEVEL: usize = BASIC_PAGE_SIZE.ilog2() as usize;

    /// Tries to find a zone bucket that satisfies the passed `alignment` page alignment, starting
    /// from the `min_zone_index` zone index.
    ///
    /// Returns the physical address of the found zone bucket
    /// and the index of the zone
    fn find_bucket_any(
        &mut self,
        alignment: NonZero<usize>,
        min_zone_index: usize,
    ) -> Result<(PhysAddr, usize), PmmError> {
        for i in min_zone_index..self.zones.len() {
            // Try finding a node that satisfies the page wise alignment
            if let Some(node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data.0 % (BASIC_PAGE_SIZE * alignment.get()) == 0)
            {
                // TODO: Remove this unwrap
                // Save the nodes address so we can return it
                let ret = node.1.data;
                // Remove the node from the zone, and push it to the freelist
                self.pop_from_zone(i, node.0);
                return Ok((ret, i));
            }
        }

        Err(PmmError::NoAvailableBlock)
    }

    /// Tries to find a zone bucket that contains the passed `addr`, starting from the
    /// `min_zone_index`
    ///
    /// Returns the index of the zone where the bucket was found
    fn find_bucket_at(&mut self, addr: PhysAddr, start_index: usize) -> Result<usize, PmmError> {
        // Try finding a node that contains the passed `addr`
        for i in start_index..self.zones.len() {
            let bucket_size = Self::index_to_bucket_size(i);
            if let Some(node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data <= addr && addr < PhysAddr(node.1.data.0 + bucket_size))
            {
                // TODO: Remove this unwrap
                // Remove the node from the zone, and push it to the freelist
                self.pop_from_zone(i, node.0);
                return Ok(i);
            }
        }

        Err(PmmError::NoAvailableBlock)
    }

    /// Splits the passed `addr` into the freelist, starting from the `bucket_index` and going
    /// down (i.e. The opposite of coalescing)
    fn disband(&mut self, mut addr: PhysAddr, bucket_index: usize, start_index: usize) {
        // We the bucket index minus 1 (i.e. the index where the block was found minus 1) until the
        // minimum zone index (the zone index of the amount of pages we're trying to allocate)
        for i in start_index..bucket_index {
            let buddy_addr = Self::get_buddy_addr(addr, i);
            self.push_to_zone(buddy_addr, i).unwrap();
            addr = Self::determine_next_zone_bucket_addr(addr, buddy_addr);
        }
    }

    /// Coalesces the passed `addr` into the freelist, starting from the `min_zone_index` and going
    /// up
    fn coalesce(&mut self, mut addr: PhysAddr, start_index: usize) {
        let mut i = start_index;
        loop {
            // We have nothing to do in a non existing zone
            if i == self.zones.len() {
                return;
            }

            // Check if this address's buddy is in the zone.
            let buddy_addr = Self::get_buddy_addr(addr, i);
            if let Some(buddy_node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data == buddy_addr)
            {
                // If the buddy is here, then we can coalesce. Logically this means combining the
                // two to a node in the next zone level.
                // What we do is just remove the buddy from the zone, and then after we finsihed
                // coalescing withg each level, we just push a node to the final level
                self.pop_from_zone(i, buddy_node.0);
                addr = Self::determine_next_zone_bucket_addr(addr, buddy_addr);
            } else {
                // If the buddy isn't here then we can't coalesce anymore so just break
                break;
            }

            i += 1;
        }

        self.push_to_zone(addr, i).unwrap();
    }

    /// Returns the buddy address of the passed `addr` in the passed `zone_index`
    ///
    /// NOTE: This method assumes that the passed `addr` belongs to the passed `zone_index`. An
    /// invalid buddy address will be returned if this is not the case.
    #[inline]
    const fn get_buddy_addr(addr: PhysAddr, zone_index: usize) -> PhysAddr {
        utils::sanity_assert!(addr.0 % Self::index_to_bucket_size(zone_index) == 0);
        let bucket_size = Self::index_to_bucket_size(zone_index);

        if addr.0 % (bucket_size * 2) == 0 {
            PhysAddr(addr.0 + bucket_size)
        } else {
            PhysAddr(addr.0 - bucket_size)
        }
    }

    /// Returns the address of the next zone bucket, given the passed `addr` and `buddy_addr`
    ///
    /// NOTE: This method assumes that the passed `addr` and `buddy_addr` are buddies. An invalid
    /// address will be returned if this is not the case.
    #[inline]
    fn determine_next_zone_bucket_addr(addr: PhysAddr, buddy_addr: PhysAddr) -> PhysAddr {
        core::cmp::min(addr, buddy_addr)
    }

    /// Pops a node from the freelist and pushes it to the zone at the passed `zone_index` and
    /// `buddy_index`
    #[inline]
    fn pop_from_zone(&mut self, zone_index: usize, buddy_index: usize) {
        let node = Box::into_non_null(self.zones[zone_index].remove_at(buddy_index).unwrap());
        unsafe { self.freelist.push_node(node) };
    }

    /// Pushes the passed `buddy_addr` to the zone at the passed `zone_index`
    fn push_to_zone(&mut self, buddy_addr: PhysAddr, zone_index: usize) -> Result<(), PmmError> {
        // TODO: Make this a const or something?
        let nodes_ptr_freelist_bucket = Self::index_to_bucket_size(self.freelist_refill_zone_index)
            / core::mem::size_of::<Node<PhysAddr>>();
        // If we need to perform emergency allocation
        utils::sanity_assert!(self.freelist.len() >= self.zones.len());
        if self.freelist.len() == self.zones.len() {
            let (buff_phys_addr, _) = self.find_bucket_any(
                unsafe { NonZero::new_unchecked(1) },
                self.freelist_refill_zone_index,
            )?;
            let ptr = NonNull::without_provenance(
                NonZero::new(buff_phys_addr.add_hhdm_offset().0).unwrap(),
            );

            for i in 0..nodes_ptr_freelist_bucket {
                unsafe { self.freelist.push_node(ptr.add(i)) };
            }
        }

        // Move the node from the freelist to `zones[zone_index]`
        let mut buddy = self.freelist.pop_node().unwrap();
        buddy.data = buddy_addr;
        unsafe {
            self.zones[zone_index].push_node(Box::into_non_null(buddy));
        }

        Ok(())
    }

    /// Checks if the passed `addr` is aligned to the passed `zone_index` (i.e. checks if the
    /// address can be a valid bucket in the zone)
    const fn check_address_alignment(addr: PhysAddr, zone_index: usize) -> Result<(), PmmError> {
        let bucket_size = Self::index_to_bucket_size(zone_index);
        if addr.0 % bucket_size != 0 {
            return Err(PmmError::InvalidAlignment);
        }

        Ok(())
    }

    /// Returns the size of the bucket at the passed `i` index
    #[inline]
    const fn index_to_bucket_size(i: usize) -> usize {
        2_usize.pow(Self::index_to_level(i) as u32)
    }

    /// Converts the passed `i` zone index to it's zone level
    #[inline]
    const fn index_to_level(i: usize) -> usize {
        i + Self::MIN_ZONE_LEVEL
    }

    /// Returns the zone index of the bucket size of the passed `level`
    #[inline]
    const fn level_to_index(level: usize) -> usize {
        level - Self::MIN_ZONE_LEVEL
    }

    /// Returns the zone index of the bucket size of the passed `bucket_size`
    #[inline]
    const fn bucket_size_to_index(bucket_size: usize) -> usize {
        utils::sanity_assert!(bucket_size.is_power_of_two());
        Self::level_to_index(bucket_size.ilog2() as usize)
    }

    /// Converts the passed `page_count` to a zone index
    #[inline]
    const fn page_count_to_index(page_count: NonZero<usize>) -> usize {
        Self::bucket_size_to_index(page_count.get() * BASIC_PAGE_SIZE)
    }

    fn break_into_buckets_n_free(&mut self, addr: PhysAddr, page_count: NonZero<usize>) {
        let upper_bound =
            Self::page_count_to_index(page_count.checked_next_power_of_two().unwrap());
        let mut page_count: usize = page_count.get();

        // Set pointers to the start and end of the memory region
        let mut low_ptr = addr.0;
        let mut high_ptr = addr.0 + (page_count * BASIC_PAGE_SIZE);

        for i in 0..upper_bound {
            let bucket_size = Self::index_to_bucket_size(i);
            // If the current low ptr isn't aligned to the next zone
            if low_ptr % (bucket_size * 2) != 0 && page_count != 0 {
                //println!("low_ptr: {:#x}, bucket_size: {:#x}", low_ptr, bucket_size);
                unsafe {
                    self.free(
                        PhysAddr(low_ptr),
                        NonZero::new(bucket_size / BASIC_PAGE_SIZE).unwrap(),
                    )
                    .unwrap()
                };
                low_ptr += bucket_size;
                page_count -= bucket_size / BASIC_PAGE_SIZE;
            }

            // If the high ptr would become aligned to the next zone if we would've allocated now
            if (high_ptr - bucket_size) % (bucket_size * 2) == 0 && page_count != 0 {
                //println!("high_ptr: {:#x}, bucket_size: {:#x}", high_ptr, bucket_size);
                high_ptr -= bucket_size;
                unsafe {
                    self.free(
                        PhysAddr(high_ptr),
                        NonZero::new(bucket_size / BASIC_PAGE_SIZE).unwrap(),
                    )
                    .unwrap()
                };
                page_count -= bucket_size / BASIC_PAGE_SIZE;
            }
        }
    }

    #[inline]
    const fn calculate_initial_buffer_size(max_zone_level: usize) -> usize {
        // TODO: Might improve efficiency if we allocate a bit more than the minimum?
        // XXX: Might need to take into account padding, but for now I think this is fine since
        // they have the same size. Otherwise we would have to take into account the padding to add
        // in between to make it aligned
        let zones_size = max_zone_level * core::mem::size_of::<StackList<PhysAddr>>();
        let min_init_nodes_size = max_zone_level * core::mem::size_of::<Node<PhysAddr>>();

        // size of zones + size of minimum initial nodes for the freelist + padding to align the
        // zones to the size of `Node<PhysAddr>` + padding to align everything to a page
        (zones_size
            + min_init_nodes_size
            + (max_zone_level % core::mem::align_of::<Node<PhysAddr>>())
            + (BASIC_PAGE_SIZE - 1))
            / BASIC_PAGE_SIZE
    }

    fn init_freelist(
        &mut self,
        mem_map: &[&limine::memory_map::Entry],
        page_count: NonZero<usize>,
    ) -> (PhysAddr, NonZero<usize>) {
        let max_zone_level =
            Self::page_count_to_index(page_count.checked_next_power_of_two().unwrap()) + 1;
        let initial_buffer_page_count = Self::calculate_initial_buffer_size(max_zone_level);

        let entry = mem_map.iter().find(|&entry| matches!(entry.entry_type, EntryType::USABLE if entry.length as usize >= initial_buffer_page_count * BASIC_PAGE_SIZE)).unwrap();

        let mut ptr: *mut StackList<PhysAddr> =
            core::ptr::without_provenance_mut(PhysAddr(entry.base as usize).add_hhdm_offset().0);

        for i in 0..max_zone_level {
            unsafe { ptr.add(i).write(StackList::new()) };
        }

        self.zones = unsafe { from_raw_parts_mut(ptr, max_zone_level) };
        self.freelist_refill_zone_index = Self::page_count_to_index(
            NonZero::new(
                (max_zone_level * core::mem::size_of::<Node<PhysAddr>>() + (BASIC_PAGE_SIZE - 1))
                    / BASIC_PAGE_SIZE,
            )
            .unwrap()
            .checked_next_power_of_two()
            .unwrap(),
        );

        {
            ptr = unsafe { ptr.add(max_zone_level) };
            let align_offset = ptr.align_offset(core::mem::align_of::<Node<PhysAddr>>());
            let ptr = unsafe { ptr.byte_add(align_offset).cast::<Node<PhysAddr>>() };

            let count = (initial_buffer_page_count * BASIC_PAGE_SIZE
                - align_offset
                - size_of_val(self.zones))
                / core::mem::size_of::<Node<PhysAddr>>();

            for i in 0..count {
                unsafe {
                    self.freelist.push_node(NonNull::new(ptr.add(i)).unwrap());
                }
            }
        }

        (
            PhysAddr(entry.base as usize),
            NonZero::new(initial_buffer_page_count).unwrap(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_utility_functions() {
        // Test index_to_bucket_size
        assert_eq!(BuddyAllocator::index_to_bucket_size(0), BASIC_PAGE_SIZE);
        assert_eq!(BuddyAllocator::index_to_bucket_size(1), BASIC_PAGE_SIZE * 2);
        assert_eq!(BuddyAllocator::index_to_bucket_size(2), BASIC_PAGE_SIZE * 4);
        assert_eq!(BuddyAllocator::index_to_bucket_size(3), BASIC_PAGE_SIZE * 8);

        // Test page_count_to_index
        assert_eq!(
            BuddyAllocator::page_count_to_index(unsafe { NonZero::new_unchecked(1) }),
            0
        );
        assert_eq!(
            BuddyAllocator::page_count_to_index(unsafe { NonZero::new_unchecked(2) }),
            1
        );
        assert_eq!(
            BuddyAllocator::page_count_to_index(unsafe { NonZero::new_unchecked(4) }),
            2
        );

        // Test get_buddy_addr
        let addr1 = PhysAddr(0x1000);
        let addr2 = PhysAddr(0x4000);
        let addr3 = PhysAddr(0x6000);
        let addr4 = PhysAddr(0x10000);
        let addr5 = PhysAddr(0x0);

        let buddy_addr1 = BuddyAllocator::get_buddy_addr(addr1, 0);
        let buddy_addr2 = BuddyAllocator::get_buddy_addr(addr2, 1);
        let buddy_addr3 = BuddyAllocator::get_buddy_addr(addr3, 1);
        let buddy_addr4 = BuddyAllocator::get_buddy_addr(addr4, 4);
        let buddy_addr5 = BuddyAllocator::get_buddy_addr(addr5, 0);

        assert_eq!(buddy_addr1, PhysAddr(0x0000));
        assert_eq!(buddy_addr2, PhysAddr(0x6000));
        assert_eq!(buddy_addr3, PhysAddr(0x4000));
        assert_eq!(buddy_addr4, PhysAddr(0x0000));
        assert_eq!(buddy_addr5, PhysAddr(0x1000));

        // Test determine_next_zone_bucket_addr
        assert_eq!(
            BuddyAllocator::determine_next_zone_bucket_addr(addr1, buddy_addr1),
            buddy_addr1
        );
        assert_eq!(
            BuddyAllocator::determine_next_zone_bucket_addr(addr2, buddy_addr2),
            addr2
        );
        assert_eq!(
            BuddyAllocator::determine_next_zone_bucket_addr(addr3, buddy_addr3),
            buddy_addr3
        );
        assert_eq!(
            BuddyAllocator::determine_next_zone_bucket_addr(addr4, buddy_addr4),
            buddy_addr4
        );
        assert_eq!(
            BuddyAllocator::determine_next_zone_bucket_addr(addr5, buddy_addr5),
            addr5
        );
    }

    #[test_case]
    fn test_alloc_n_frees() {
        let allocator = unsafe {
            #[allow(static_mut_refs)]
            &mut BUDDY_ALLOCATOR
        };

        let addr0 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(2)
            })
            .unwrap();
        let addr1 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(2) }, unsafe {
                NonZero::new_unchecked(2)
            })
            .unwrap();
        let addr2 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(7)
            })
            .unwrap();
        unsafe { allocator.free(addr0, NonZero::new_unchecked(2)).unwrap() };
        unsafe { allocator.free(addr1, NonZero::new_unchecked(2)).unwrap() };
        for _ in 0..12 {
            let addr = allocator
                .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                    NonZero::new_unchecked(1)
                })
                .unwrap();
            assert_eq!(
                allocator.is_page_free(addr, unsafe { NonZero::new_unchecked(1) }),
                Ok(false)
            );
            unsafe { allocator.free(addr, NonZero::new_unchecked(1)).unwrap() };
            assert_eq!(
                allocator.is_page_free(addr, unsafe { NonZero::new_unchecked(1) }),
                Ok(true)
            );
        }

        assert_eq!(
            allocator.is_page_free(addr2, unsafe { NonZero::new_unchecked(7) }),
            Ok(false)
        );
        unsafe { allocator.free(addr2, NonZero::new_unchecked(7)).unwrap() };
        assert_eq!(
            allocator.is_page_free(addr2, unsafe { NonZero::new_unchecked(7) }),
            Ok(true)
        );

        let addr1 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(2)
            })
            .unwrap();
        let addr2 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(8)
            })
            .unwrap();
        unsafe { allocator.free(addr1, NonZero::new_unchecked(2)).unwrap() };
        let addr3 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(8)
            })
            .unwrap();
        let addr4 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(1)
            })
            .unwrap();
        unsafe { allocator.free(addr2, NonZero::new_unchecked(8)).unwrap() };
        unsafe { allocator.free(addr3, NonZero::new_unchecked(8)).unwrap() };
        unsafe { allocator.free(addr4, NonZero::new_unchecked(1)).unwrap() };
    }

    #[test_case]
    fn test_errors() {
        let allocator = unsafe {
            #[allow(static_mut_refs)]
            &mut BUDDY_ALLOCATOR
        };

        let addr0 = allocator
            .alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(2)
            })
            .unwrap();

        // can't compare with a specific error since it might return Unaligned or NoAvailableBlock,
        unsafe { allocator.free(addr0, NonZero::new_unchecked(2)).unwrap() };
        unsafe {
            assert_eq!(
                allocator.free(addr0, NonZero::new_unchecked(2)),
                Err(PmmError::FreeOfAlreadyFree)
            )
        };

        assert_eq!(
            allocator.alloc_any(unsafe { NonZero::new_unchecked(1) }, unsafe {
                NonZero::new_unchecked(usize::MAX)
            }),
            Err(PmmError::NoAvailableBlock)
        );
    }

    // TODO: Need to test alloc_at
}
