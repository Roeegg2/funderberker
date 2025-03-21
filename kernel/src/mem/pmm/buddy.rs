
use core::{alloc::Layout, num::NonZero, ptr::NonNull, slice::from_raw_parts_mut};

use alloc::boxed::Box;
use limine::memory_map::EntryType;
use utils::collections::stacklist::{Node, StackList};

use crate::{arch::BASIC_PAGE_SIZE, boot::limine::get_page_count_from_mem_map, mem::{PageId, PhysAddr}};

use super::{PmmAllocator, PmmError};

/// Singleton instance of the buddy allocator
pub(super) static mut BUDDY_ALLOCATOR: BuddyAllocator = BuddyAllocator {
    zones: &mut [],
    freelist: StackList::new(),
};

/// A buddy allocator for the PMM
pub(super) struct BuddyAllocator<'a> {
    zones: &'a mut [StackList<PhysAddr>],
    freelist: StackList<PhysAddr>,
}

impl<'a> PmmAllocator for BuddyAllocator<'a> {
    fn alloc_at(&mut self, addr: crate::mem::PhysAddr, page_count: usize) -> Result<(), PmmError> {
        // We can't allocate 0 pages
        if page_count == 0 {
            return Err(PmmError::NoAvailableBlock);
        }
        
        let zone_bucket_size = page_count.next_power_of_two() * BASIC_PAGE_SIZE;
        // Make sure the address is aligned to the current page count
        if addr.0 % zone_bucket_size != 0 {
            return Err(PmmError::InvalidAlignment);
        }
        // Get the "zone level" using the page count, and then subtract `Self::ZONE_LOWER_BOUND` to
        // get the index into the `zones` field

        let min_zone_index = zone_bucket_size.ilog2() as usize - Self::ZONE_LOWER_BOUND;
        let bucket_index = self.find_at_zone_bucket(addr, min_zone_index, zone_bucket_size)?;
        self.disband(addr, bucket_index, min_zone_index);

        Ok(())
    }

    fn alloc_any(&mut self, alignment: PageId, page_count: usize) -> Result<PhysAddr, PmmError> {
        if page_count == 0 {
            return Err(PmmError::NoAvailableBlock);
        } 

        if alignment == 0 {
            return Err(PmmError::InvalidAlignment);
        }

        let zone_bucket_size = page_count.next_power_of_two() * BASIC_PAGE_SIZE;

        let min_zone_index = zone_bucket_size.ilog2() as usize - Self::ZONE_LOWER_BOUND;
        let (addr, bucket_index) = self.find_zone_bucket(alignment, min_zone_index)?;
        println!("Allocating at: {:#x} with page count: {:#x}", addr.0, page_count);
        self.disband(addr, bucket_index, min_zone_index);

        Ok(addr)
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, PmmError> {
        // TODO: Optimize, we might be able to skip some checking of zones, depending on `addr` (if
        // it's not aligned to zone 'nth') 
        for i in 0..self.zones.len() {
            for bucket in self.zones[i].iter() {
                if  *bucket == addr {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    // TODO: Maybe do it the old way, by breaking it down into blocks and then coalescing instead
    // of just everything as page sizes?
    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), PmmError> {
        if self.is_page_free(addr)? {
            return Err(PmmError::FreeOfAlreadyFree);
        }

        if page_count == 0 {
            return Err(PmmError::NoAvailableBlock);
        }

        let zone_bucket_size = page_count.next_power_of_two() * BASIC_PAGE_SIZE;
        if addr.0 % zone_bucket_size != 0 {
            return Err(PmmError::InvalidAlignment);
        }

        let min_zone_index = zone_bucket_size.ilog2() as usize - Self::ZONE_LOWER_BOUND;
        self.coalesce(addr, min_zone_index);

        Ok(())
    }

    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        let page_count = get_page_count_from_mem_map(mem_map);

        // TODO: Set the `page_count` and check to make sure the address's passed in `free` and
        // `alloc_at` are within the bounds of the memory map

        #[allow(static_mut_refs)]
        let freelist_entry_addr = unsafe {BUDDY_ALLOCATOR.init_freelist(mem_map, page_count)};

        for entry in mem_map.iter() {
            match entry.entry_type {
                EntryType::USABLE | EntryType::BOOTLOADER_RECLAIMABLE => {
                    let addr = PhysAddr(entry.base as usize);
                    unsafe {
                        #[allow(static_mut_refs)]
                        BUDDY_ALLOCATOR.break_into_buckets_n_free(addr, entry.length as usize);
                    };
                }
                _ => continue,
            }
        }

        // TODO: Mark the 2 pages from the freelist entry as taken
        unsafe {
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR.alloc_at(PhysAddr(freelist_entry_addr.0), 1).unwrap();
            println!("Freelist entry addr: {:#x}", freelist_entry_addr.0);
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR.alloc_at(PhysAddr(freelist_entry_addr.0 + BASIC_PAGE_SIZE), 1).unwrap();
            println!("Freelist entry addr: {:#x}", freelist_entry_addr.0 + BASIC_PAGE_SIZE);
        }
    }
}

impl<'a> BuddyAllocator<'a> {
    /// The lowest possible zone level (the zone level of `BASIC_PAGE_SIZE`)
    const ZONE_LOWER_BOUND: usize = BASIC_PAGE_SIZE.ilog2() as usize;

    // TODO: Maybe deremine a function for this instead of just using 0?
    /// From what zone index we should refill the freelist
    const FREELIST_REFILL_ZONE_INDEX: usize = 0;

    /// The number of nodes per freelist bucket
    const NODES_PER_FREELIST_BUCKET: usize = BASIC_PAGE_SIZE / Layout::new::<Node<PhysAddr>>().pad_to_align().size();

    /// Tries to find a zone bucket that satisfies the passed `alignment` page alignment, starting
    /// from the `min_zone_index` zone index. 
    ///
    /// Returns the physical address of the found zone bucket
    /// and the index of the zone
    fn find_zone_bucket(&mut self, alignment: PageId, min_zone_index: usize) -> Result<(PhysAddr, usize), PmmError> {
        for i in min_zone_index..self.zones.len() {
            // Try finding a node that satisfies the page wise alignment
            if let Some(node) = self.zones[i].iter_node().enumerate().find(|&node| {
                node.1.data.0 % (BASIC_PAGE_SIZE * alignment) == 0
            }) {
                // TODO: Remove this unwrap
                // Save the nodes address so we can return it
                let ret = node.1.data;
                // Remove the node from the zone, and push it to the freelist
                let node = Box::into_non_null(self.zones[i].remove_at(node.0).unwrap());
                unsafe {self.freelist.push_node(node)};
                return Ok((ret, i));
            }
        }

        Err(PmmError::NoAvailableBlock)
    }

    /// Tries to find a zone bucket that contains the passed `addr`, starting from the
    /// `min_zone_index`
    ///
    /// Returns the index of the zone where the bucket was found
    fn find_at_zone_bucket(&mut self, addr: PhysAddr, min_zone_index: usize, zone_bucket_size: usize) -> Result<usize, PmmError> {
        // Try finding a node that contains the passed `addr`
        for i in min_zone_index..self.zones.len() {
            if let Some(node) = self.zones[i].iter_node().enumerate().find(|&node| {
                node.1.data <= addr && addr < PhysAddr(node.1.data.0 + zone_bucket_size)
            }) {
                // TODO: Remove this unwrap
                // Remove the node from the zone, and push it to the freelist
                let node = Box::into_non_null(self.zones[i].remove_at(node.0).unwrap());
                unsafe {self.freelist.push_node(node)};
                return Ok(i);
            }
        }

        Err(PmmError::NoAvailableBlock)
    }

    /// Splits the passed `addr` into the freelist, starting from the `bucket_index` and going
    /// down (i.e. The opposite of coalescing)
    fn disband(&mut self, addr: PhysAddr, bucket_index: usize, min_zone_index: usize) {
        for i in (min_zone_index..bucket_index).rev() {
            let buddy_addr = Self::get_buddy_addr(addr, i);
            //println!("Buddy addr: {:#x} addr: {:#x} bucket_index: {:#x} min_zone_index: {:#x}", buddy_addr.0, addr.0, bucket_index, min_zone_index);
            self.push_from_freelist(buddy_addr, i).unwrap();
        }
    }

    /// Coalesces the passed `addr` into the freelist, starting from the `min_zone_index` and going
    /// up 
    fn coalesce(&mut self, mut addr: PhysAddr, min_zone_index: usize) {
        //println!("Coalescing: {:#x}", addr.0);
        let mut i = min_zone_index;
        loop {
            if i == self.zones.len() {
                i -= 1;
                break;
            }

            // Check if this address's buddy is in the zone.
            let buddy_addr = Self::get_buddy_addr(addr, i);
            if let Some(buddy_node) = self.zones[i].iter_node().enumerate().find(|&node| {
                node.1.data == buddy_addr
            }) {
                // If the buddy is here, then we can coalesce. Logically this means combining the
                // two and pushing them to the next zone level.
                // What we do is just remove the buddy from the zone, and then after we finsihed
                // coalescing withg each level, we just push a node to the final level
                addr = Self::determine_next_zone_bucket_addr(addr, buddy_addr);
                let node = Box::into_non_null(self.zones[i].remove_at(buddy_node.0).unwrap());
                unsafe {self.freelist.push_node(node)};
            } else {
                // If the buddy isn't here then we can't coalesce anymore so just break
                break;
            }
        }

        self.push_from_freelist(addr, i).unwrap();
    }

    // TODO: Maybe rewrite this?
    #[inline]
    fn get_buddy_addr(addr: PhysAddr, zone_index: usize) -> PhysAddr {
        let zone_bucket_size = 2_usize.pow((zone_index + Self::ZONE_LOWER_BOUND) as u32);
        
        if addr.0 % (zone_bucket_size * 2) == 0 {
            PhysAddr(addr.0 + zone_bucket_size)
        } else {
            PhysAddr(addr.0 - zone_bucket_size)
        }
    }

    #[inline]
    fn determine_next_zone_bucket_addr(addr: PhysAddr, buddy_addr: PhysAddr) -> PhysAddr {
        core::cmp::min(addr, buddy_addr)
    }

    fn push_from_freelist(&mut self, addr: PhysAddr, zone_index: usize) -> Result<(), PmmError> {
        // If we need to perform emergency allocation
        if self.freelist.len() == self.zones.len() {
            let (buff_phys_addr, _) = self.find_zone_bucket(1, Self::FREELIST_REFILL_ZONE_INDEX)?;
            let ptr = NonNull::without_provenance(NonZero::new(buff_phys_addr.add_hhdm_offset().0).unwrap());

            for i in 0..Self::NODES_PER_FREELIST_BUCKET {
                unsafe { self.freelist.push_node(ptr.add(i)) };
            }
        }

        let mut buddy = self.freelist.pop_node().unwrap();
        buddy.data = addr;
        unsafe {
            self.zones[zone_index].push_node(Box::into_non_null(buddy));
        }

        Ok(())
    }

    fn break_into_buckets_n_free(&mut self, addr: PhysAddr, mut bytes_count: usize) {
        let mut low_ptr = addr.0;
        let mut high_ptr = addr.0 + bytes_count;

        let upper_bound = bytes_count.ilog2() as usize - Self::ZONE_LOWER_BOUND;

        for i in 0..upper_bound {
            let bucket_size = 2_usize.pow((i + Self::ZONE_LOWER_BOUND) as u32);
            if low_ptr % (bucket_size * 2) != 0 {
                unsafe {self.free(PhysAddr(low_ptr), bucket_size / BASIC_PAGE_SIZE).unwrap()};
                low_ptr += bucket_size;
                bytes_count -= bucket_size;
            }

            if (high_ptr - bucket_size) % (bucket_size * 2) == 0 {
                high_ptr -= bucket_size;
                unsafe {self.free(PhysAddr(high_ptr), bucket_size / BASIC_PAGE_SIZE).unwrap()};
                bytes_count -= bucket_size;
            }
        }

        if bytes_count != 0 {
            let bucket_size = 2_usize.pow((upper_bound + Self::ZONE_LOWER_BOUND) as u32);
            let page_count = ((bytes_count / bucket_size) * bucket_size) / BASIC_PAGE_SIZE;
            bytes_count -= page_count * BASIC_PAGE_SIZE;
            unsafe {self.free(PhysAddr(low_ptr), page_count).unwrap()};
        }

        assert_eq!(bytes_count, 0);
    }

    fn init_freelist(&mut self, mem_map: &[&limine::memory_map::Entry], page_count: usize) -> PhysAddr {
        let entry = mem_map.iter().find(|&entry| {
            match entry.entry_type {
                EntryType::BOOTLOADER_RECLAIMABLE | EntryType::USABLE if entry.length as usize >= 2 * BASIC_PAGE_SIZE => true,
                _ => false,
            }
        }).unwrap();

        let zones_ptr: *mut StackList<PhysAddr> = 
            core::ptr::without_provenance_mut(PhysAddr(entry.length as usize).add_hhdm_offset().0);
        
        let zone_level_upper_bound = (page_count * BASIC_PAGE_SIZE).ilog2() as usize - Self::ZONE_LOWER_BOUND + 1;
        self.zones = unsafe {
            from_raw_parts_mut(zones_ptr, zone_level_upper_bound)
        };

        let add = zones_ptr.cast::<u8>().align_offset(align_of::<Node<PhysAddr>>());
        let freelist_entries_ptr = unsafe {zones_ptr.add(zone_level_upper_bound).byte_add(add).cast::<Node<PhysAddr>>()};

        let freelist_entries_count = ((2 * BASIC_PAGE_SIZE) - (freelist_entries_ptr.addr() - zones_ptr.addr())) / Layout::new::<Node<PhysAddr>>().pad_to_align().size();

        println!("this is the add: {:#x}", add);
        println!("thisis the size: {:#x}", Layout::new::<Node<PhysAddr>>().pad_to_align().size());
        println!("this is freelist count: {:#x}", freelist_entries_count);

        for i in 0..freelist_entries_count {
            unsafe {self.freelist.push_node(NonNull::new(freelist_entries_ptr.add(i)).unwrap())};
        }

        PhysAddr(entry.base as usize)
    }
}
