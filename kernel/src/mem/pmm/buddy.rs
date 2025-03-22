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
#[derive(Debug)]
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
        
        let zone_index = Self::page_count_to_index(page_count);
        {
            let bucket_size = Self::index_to_bucket_size(zone_index);
            if addr.0 % bucket_size != 0 {
                return Err(PmmError::InvalidAlignment);
            }
        }

        let zone_index = Self::page_count_to_index(page_count);
        let bucket_index = self.find_bucket_at(addr, zone_index)?;
        self.disband(addr, bucket_index, zone_index);

        Ok(())
    }

    fn alloc_any(&mut self, alignment: PageId, page_count: usize) -> Result<PhysAddr, PmmError> {
        if page_count == 0 {
            return Err(PmmError::NoAvailableBlock);
        } 

        if alignment == 0 {
            return Err(PmmError::InvalidAlignment);
        }

        let zone_index = Self::page_count_to_index(page_count);
        let (addr, bucket_index) = self.find_bucket_any(alignment, zone_index)?;
        self.disband(addr, bucket_index, zone_index);

        Ok(addr)
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, PmmError> {
        // TODO: Optimize, we might be able to skip some checking of zones, depending on `addr` (if
        // it's not aligned to zone 'nth') 
        for i in 0..self.zones.len() {
            for bucket in self.zones[i].iter() {
                let bucket_size = Self::index_to_bucket_size(i);
                if *bucket <= addr && addr < PhysAddr(bucket.0 + bucket_size) {
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

        let zone_index = Self::page_count_to_index(page_count);
        {
            let bucket_size = Self::index_to_bucket_size(zone_index);
            if addr.0 % bucket_size != 0 {
                return Err(PmmError::InvalidAlignment);
            }
        }

        self.coalesce(addr, zone_index);

        Ok(())
    }

    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        let total_page_count = get_page_count_from_mem_map(mem_map);

        // XXX: If page count is 0, then we can't do anything
        // TODO: Set the `page_count` and check to make sure the address's passed in `free` and
        // `alloc_at` are within the bounds of the memory map

        #[allow(static_mut_refs)]
        let freelist_entry_addr = unsafe {BUDDY_ALLOCATOR.init_freelist(mem_map, total_page_count)};

        for entry in mem_map.iter() {
            match entry.entry_type {
                EntryType::USABLE => {
                    let page_count = entry.length as usize / BASIC_PAGE_SIZE;
                    let addr = PhysAddr(entry.base as usize);
                    unsafe {
                        #[allow(static_mut_refs)]
                        BUDDY_ALLOCATOR.break_into_buckets_n_free(addr, page_count);
                    };
                }
                _ => continue,
            }
        }
        println!("mem map stuff went fine");

        // TODO: Mark the 2 pages from the freelist entry as taken
        unsafe {
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR.alloc_at(PhysAddr(freelist_entry_addr.0), 1).unwrap();
            println!("Freelist entry addr: {:#x}", freelist_entry_addr.0);
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR.alloc_at(PhysAddr(freelist_entry_addr.0 + BASIC_PAGE_SIZE), 1).unwrap();
            println!("Freelist entry addr: {:#x}", freelist_entry_addr.0 + BASIC_PAGE_SIZE);
        }

        println!("also freelist went fine");
    }
}

impl<'a> BuddyAllocator<'a> {
    /// The lowest possible zone level (the zone level of `BASIC_PAGE_SIZE`)
    const MIN_ZONE_LEVEL: usize = BASIC_PAGE_SIZE.ilog2() as usize;

    // TODO: Maybe deremine a function for this instead of just using 0?
    /// From what zone index we should refill the freelist
    const FREELIST_REFILL_ZONE_INDEX: usize = 0;

    // TODO: Swap out the use of Layout here?
    /// The number of nodes per freelist bucket
    const NODES_PER_FREELIST_BUCKET: usize = Self::index_to_bucket_size(Self::FREELIST_REFILL_ZONE_INDEX) / Layout::new::<Node<PhysAddr>>().pad_to_align().size();

    /// Tries to find a zone bucket that satisfies the passed `alignment` page alignment, starting
    /// from the `min_zone_index` zone index. 
    ///
    /// Returns the physical address of the found zone bucket
    /// and the index of the zone
    fn find_bucket_any(&mut self, alignment: PageId, min_zone_index: usize) -> Result<(PhysAddr, usize), PmmError> {
        for i in min_zone_index..self.zones.len() {
            // Try finding a node that satisfies the page wise alignment
            if let Some(node) = self.zones[i].iter_node().enumerate().find(|&node|
                node.1.data.0 % (BASIC_PAGE_SIZE * alignment) == 0
            ) {
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
            if let Some(node) = self.zones[i].iter_node().enumerate().find(|&node|
                node.1.data <= addr && addr < PhysAddr(node.1.data.0 + bucket_size)
            ) {
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
    fn disband(&mut self, addr: PhysAddr, bucket_index: usize, start_index: usize) {
        // We the bucket index minus 1 (i.e. the index where the block was found minus 1) until the
        // minimum zone index (the zone index of the amount of pages we're trying to allocate)
        for i in (start_index..bucket_index).rev() {
            let buddy_addr = Self::get_buddy_addr(addr, i);
            self.push_to_zone(buddy_addr, i).unwrap();
        }
    }

    /// Coalesces the passed `addr` into the freelist, starting from the `min_zone_index` and going
    /// up 
    fn coalesce(&mut self, mut addr: PhysAddr, start_index: usize) {
        let mut i = start_index;
        loop {
            debug_assert_ne!(i, self.zones.len());

            // Check if this address's buddy is in the zone.
            let buddy_addr = Self::get_buddy_addr(addr, i);
            if let Some(buddy_node) = self.zones[i].iter_node().enumerate().find(|&node|
                node.1.data == buddy_addr
            ) {
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

    // TODO: Maybe rewrite this?
    #[inline]
    const fn get_buddy_addr(addr: PhysAddr, zone_index: usize) -> PhysAddr {
        let bucket_size = Self::index_to_bucket_size(zone_index);
        
        if addr.0 % (bucket_size * 2) == 0 {
            PhysAddr(addr.0 + bucket_size)
        } else {
            PhysAddr(addr.0 - bucket_size)
        }
    }

    #[inline]
    fn determine_next_zone_bucket_addr(addr: PhysAddr, buddy_addr: PhysAddr) -> PhysAddr {
        core::cmp::min(addr, buddy_addr)
    }

    #[inline]
    fn pop_from_zone(&mut self, zone_index: usize, buddy_index: usize) {
        let node = Box::into_non_null(self.zones[zone_index].remove_at(buddy_index).unwrap());
        unsafe {self.freelist.push_node(node)};
    }

    fn push_to_zone(&mut self, buddy_addr: PhysAddr, zone_index: usize) -> Result<(), PmmError> {
        // If we need to perform emergency allocation
        if self.freelist.len() == self.zones.len() {
            let (buff_phys_addr, _) = self.find_bucket_any(1, Self::FREELIST_REFILL_ZONE_INDEX)?;
            let ptr = NonNull::without_provenance(NonZero::new(buff_phys_addr.add_hhdm_offset().0).unwrap());

            for i in 0..Self::NODES_PER_FREELIST_BUCKET {
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

    #[inline(always)]
    const fn index_to_bucket_size(i: usize) -> usize {
        2_usize.pow(Self::index_to_level(i) as u32)
    }

    #[inline(always)]
    const fn index_to_level(i: usize) -> usize {
        i + Self::MIN_ZONE_LEVEL
    }
    
    #[inline(always)]
    const fn level_to_index(level: usize) -> usize {
        level - Self::MIN_ZONE_LEVEL
    }

    #[inline(always)]
    const fn bucket_size_to_index(bucket_size: usize) -> usize {
        debug_assert!(bucket_size.is_power_of_two());
        Self::level_to_index(bucket_size.ilog2() as usize)
    }

    #[inline(always)]
    const fn page_count_to_index(page_count: usize) -> usize {
        Self::bucket_size_to_index(page_count * BASIC_PAGE_SIZE)
    }

    fn break_into_buckets_n_free(&mut self, addr: PhysAddr, mut page_count: usize) {
        // Set pointers to the start and end of the memory region
        let mut low_ptr = addr.0;
        let mut high_ptr = addr.0 + (page_count * BASIC_PAGE_SIZE);

        // NOTE: Using `page_count.next_power_of_two() / 2` because we want to run up until one 
        //println!("yosh addr: {:#x}, page_count {:x}", addr.0, page_count);
        let aaa = Self::page_count_to_index(page_count.next_power_of_two() / 2);
        //println!("oka");
        for i in 0..aaa {
            let bucket_size = Self::index_to_bucket_size(i);
            // If the current low ptr isn't aligned to the next zone
            if low_ptr % (bucket_size * 2) != 0 {
                //println!("low_ptr: {:#x}, bucket_size: {:#x}", low_ptr, bucket_size);
                unsafe {self.free(PhysAddr(low_ptr), bucket_size / BASIC_PAGE_SIZE).unwrap()};
                low_ptr += bucket_size;
                page_count -= bucket_size / BASIC_PAGE_SIZE;
            }

            // If the high ptr would become aligned to the next zone if we would've allocated now
            if (high_ptr - bucket_size) % (bucket_size * 2) == 0 {
                //println!("high_ptr: {:#x}, bucket_size: {:#x}", high_ptr, bucket_size);
                high_ptr -= bucket_size;
                unsafe {self.free(PhysAddr(high_ptr), bucket_size / BASIC_PAGE_SIZE).unwrap()};
                page_count -= bucket_size / BASIC_PAGE_SIZE;
            }
        }

        // All that is left can be allocated using the highest bucket size, so allocate it.
        // Because in the previous for loop we allocated all the smaller buckets as needed, we can
        // garuntee this will be a multiple of the highest zone's bucket size
        if page_count != 0 {
            unsafe {self.free(PhysAddr(low_ptr), page_count).unwrap()};
        }
    }

    fn init_freelist(&mut self, mem_map: &[&limine::memory_map::Entry], page_count: usize) -> PhysAddr {
        // Find an entry that is free and that has at least 1 page
        let entry = mem_map.iter().find(|&entry| matches!(entry.entry_type, EntryType::USABLE if entry.length as usize >= 1 * BASIC_PAGE_SIZE)).unwrap();

        // construct a pointer to the `zones` array: Add HHDM offset to the physical address, and then cast it to a pointer        
        let zones_ptr: *mut StackList<PhysAddr> = {
            let virt_addr = PhysAddr(entry.base as usize).add_hhdm_offset();
            core::ptr::without_provenance_mut(virt_addr.0)
        };
        
        // Zone level + 1
        let zone_size = (page_count * BASIC_PAGE_SIZE).ilog2() as usize + 1;

        // Construct the zones slice
        self.zones = unsafe {
            from_raw_parts_mut(zones_ptr, zone_size)
        };

        // Get the address for the freelist entries
        let freelist_entries_ptr = {
            // Cast the pointer
            let mut ptr = zones_ptr.cast::<Node<PhysAddr>>();
            // Skip `self.zones`
            ptr = unsafe {ptr.byte_add(core::mem::size_of_val(self.zones))};
            // Align the pointer to the size of `Node<PhysAddr>`
            ptr = unsafe {ptr.byte_add(ptr.align_offset(core::mem::align_of::<Node<PhysAddr>>()))};
            ptr
        };


        // XXX: Might need to consider alignment as well?
        let freelist_entries_count = {
            let offset = freelist_entries_ptr.addr() - zones_ptr.addr();
            let bytes_amount = (1 * BASIC_PAGE_SIZE) - offset;
            //unsafe {utils::mem::memset(freelist_entries_ptr.cast::<u8>(), 0, bytes_amount)};
            bytes_amount / core::mem::size_of::<Node<PhysAddr>>()
        };

        for i in 0..freelist_entries_count {
            unsafe {self.freelist.push_node(NonNull::new(freelist_entries_ptr.add(i)).unwrap())};
        }

        PhysAddr(entry.base as usize)
    }
}
