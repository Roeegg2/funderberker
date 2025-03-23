use core::{alloc::Layout, num::NonZero, ptr::NonNull, slice::from_raw_parts_mut};

use alloc::boxed::Box;
use limine::memory_map::EntryType;
use utils::collections::stacklist::{Node, StackList};

use crate::{
    arch::BASIC_PAGE_SIZE,
    boot::limine::get_page_count_from_mem_map,
    mem::{PageId, PhysAddr},
};

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
        println!("Called free");
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

        println!(
            "Coalescing addr {:#x} with zone index {:#x}",
            addr.0, zone_index
        );
        self.coalesce(addr, zone_index);

        Ok(())
    }

    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        let total_page_count = get_page_count_from_mem_map(mem_map);

        // XXX: If page count is 0, then we can't do anything
        // TODO: Set the `page_count` and check to make sure the address's passed in `free` and
        // `alloc_at` are within the bounds of the memory map

        #[allow(static_mut_refs)]
        let freelist_entry_addr =
            unsafe { BUDDY_ALLOCATOR.init_freelist(mem_map, total_page_count) };

        for entry in mem_map.iter() {
            match entry.entry_type {
                EntryType::USABLE => {
                    let page_count = entry.length as usize / BASIC_PAGE_SIZE;
                    let addr = PhysAddr(entry.base as usize);
                    println!("addr: {:#x}, page_count: {:#x}", addr.0, page_count);
                    for i in 0..page_count {
                        unsafe {
                            #[allow(static_mut_refs)]
                            BUDDY_ALLOCATOR
                                .free(PhysAddr(addr.0 + (i * BASIC_PAGE_SIZE)), 1)
                                .unwrap();
                            //BUDDY_ALLOCATOR.break_into_buckets_n_free(addr, 1);
                        }
                    }
                    //unsafe {
                    //    #[allow(static_mut_refs)]
                    //    BUDDY_ALLOCATOR.break_into_buckets_n_free(addr, page_count);
                    //};
                }
                _ => continue,
            }
        }
        //println!("mem map stuff went fine");

        // TODO: Mark the 2 pages from the freelist entry as taken
        unsafe {
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR
                .alloc_at(PhysAddr(freelist_entry_addr.0), 1)
                .unwrap();
            println!("Freelist entry addr: {:#x}", freelist_entry_addr.0);
            #[allow(static_mut_refs)]
            BUDDY_ALLOCATOR
                .alloc_at(PhysAddr(freelist_entry_addr.0 + BASIC_PAGE_SIZE), 1)
                .unwrap();
            println!(
                "Freelist entry addr: {:#x}",
                freelist_entry_addr.0 + BASIC_PAGE_SIZE
            );
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
    const NODES_PER_FREELIST_BUCKET: usize =
        Self::index_to_bucket_size(Self::FREELIST_REFILL_ZONE_INDEX)
            / Layout::new::<Node<PhysAddr>>().pad_to_align().size();

    /// Tries to find a zone bucket that satisfies the passed `alignment` page alignment, starting
    /// from the `min_zone_index` zone index.
    ///
    /// Returns the physical address of the found zone bucket
    /// and the index of the zone
    fn find_bucket_any(
        &mut self,
        alignment: PageId,
        min_zone_index: usize,
    ) -> Result<(PhysAddr, usize), PmmError> {
        for i in min_zone_index..self.zones.len() {
            // Try finding a node that satisfies the page wise alignment
            if let Some(node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data.0 % (BASIC_PAGE_SIZE * alignment) == 0)
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
            println!("i: {:#x}, len: {:#x}", i, self.zones.len());
            debug_assert_ne!(i, self.zones.len());

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
        unsafe { self.freelist.push_node(node) };
    }

    fn push_to_zone(&mut self, buddy_addr: PhysAddr, zone_index: usize) -> Result<(), PmmError> {
        // If we need to perform emergency allocation
        if self.freelist.len() == self.zones.len() {
            let (buff_phys_addr, _) = self.find_bucket_any(1, Self::FREELIST_REFILL_ZONE_INDEX)?;
            let ptr = NonNull::without_provenance(
                NonZero::new(buff_phys_addr.add_hhdm_offset().0).unwrap(),
            );

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
        //println!("page_count: {:#x}", page_count);
        // Set pointers to the start and end of the memory region
        let mut low_ptr = addr.0;
        let mut high_ptr = addr.0 + (page_count * BASIC_PAGE_SIZE);

        //println!("top: {:#x}", Self::page_count_to_index(page_count.next_power_of_two()));
        for i in 0..Self::page_count_to_index(page_count.next_power_of_two()) {
            println!("low_ptr: {:#x}, high_ptr: {:#x}", low_ptr, high_ptr);
            let bucket_size = Self::index_to_bucket_size(i);
            // If the current low ptr isn't aligned to the next zone
            if low_ptr % (bucket_size * 2) != 0 && page_count != 0 {
                //println!("low_ptr: {:#x}, bucket_size: {:#x}", low_ptr, bucket_size);
                unsafe {
                    self.free(PhysAddr(low_ptr), bucket_size / BASIC_PAGE_SIZE)
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
                    self.free(PhysAddr(high_ptr), bucket_size / BASIC_PAGE_SIZE)
                        .unwrap()
                };
                page_count -= bucket_size / BASIC_PAGE_SIZE;
            }
        }

        // All that is left can be allocated using the highest bucket size, so allocate it.
        // Because in the previous for loop we allocated all the smaller buckets as needed, we can
        // garuntee this will be a multiple of the highest zone's bucket size
        if page_count != 0 {
            //println!("ahoy there!");
            //    //unsafe {self.free(PhysAddr(low_ptr), page_count).unwrap()};
        }
    }

    fn init_freelist(
        &mut self,
        mem_map: &[&limine::memory_map::Entry],
        page_count: usize,
    ) -> PhysAddr {
        // Find an entry that is free and that has at least 1 page
        let entry = mem_map.iter().find(|&entry| matches!(entry.entry_type, EntryType::USABLE if entry.length as usize >= 1 * BASIC_PAGE_SIZE)).unwrap();

        // construct a pointer to the `zones` array: Add HHDM offset to the physical address, and then cast it to a pointer
        let zones_ptr: *mut StackList<PhysAddr> = {
            let virt_addr = PhysAddr(entry.base as usize).add_hhdm_offset();
            core::ptr::without_provenance_mut(virt_addr.0)
        };

        // Zone level + 1
        let zone_size = Self::page_count_to_index(page_count.next_power_of_two()) + 1;

        // Construct the zones slice
        self.zones = unsafe { from_raw_parts_mut(zones_ptr, zone_size) };

        // Get the address for the freelist entries
        let freelist_entries_ptr = {
            // Cast the pointer
            let mut ptr = zones_ptr.cast::<Node<PhysAddr>>();
            // Skip `self.zones`
            ptr = unsafe { ptr.byte_add(core::mem::size_of_val(self.zones)) };
            // Align the pointer to the size of `Node<PhysAddr>`
            ptr =
                unsafe { ptr.byte_add(ptr.align_offset(core::mem::align_of::<Node<PhysAddr>>())) };
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
            unsafe {
                self.freelist
                    .push_node(NonNull::new(freelist_entries_ptr.add(i)).unwrap())
            };
        }

        PhysAddr(entry.base as usize)
    }
}

//#[cfg(test)]
//mod tests {
//use core::ptr::NonNull;
//use core::sync::atomic::{AtomicUsize, Ordering};
//
//use alloc::boxed::Box;
//use limine::memory_map::{Entry, EntryType};
//use utils::collections::stacklist::StackList;
//
//use crate::{
//    arch::BASIC_PAGE_SIZE,
//    mem::{PageId, PhysAddr},
//    pmm::{buddy::BuddyAllocator, PmmAllocator, PmmError},
//};
//
//// Mock memory map for testing
//fn create_mock_memory_map() -> [Box<Entry>; 1] {
//    [Box::new(Entry {
//        base: 0x1000,
//        length: 0x100000, // 1MB of memory
//        entry_type: EntryType::USABLE,
//        acpi_extended_attributes: 0,
//    })]
//}
//
//// Create a reference memory map for the tests
//fn create_memory_map_refs<'a>(entries: &'a [Box<Entry>]) -> Vec<&'a Entry> {
//    entries.iter().map(|e| e.as_ref()).collect()
//}
//
//// Helper to create and initialize a buddy allocator for testing
//fn setup_buddy_allocator<'a>() -> BuddyAllocator<'a> {
//    // Create mock memory map
//    let mem_map_entries = create_mock_memory_map();
//    let mem_map_refs = create_memory_map_refs(&mem_map_entries);
//
//    // Setup zones for the allocator
//    let zone_size = 20; // Arbitrary size for testing
//    let mut zones = Box::new([StackList::<PhysAddr>::new(); 20]);
//
//    let mut allocator = BuddyAllocator {
//        zones: &mut zones[..],
//        freelist: StackList::new(),
//    };
//
//    // Initialize freelist with mock nodes
//    let mut nodes = Vec::new();
//    for i in 0..100 {
//        let layout = core::alloc::Layout::new::<utils::collections::stacklist::Node<PhysAddr>>();
//        let node_box = unsafe { alloc::alloc::alloc(layout) };
//        let ptr = NonNull::new(node_box as *mut _).unwrap();
//        nodes.push(ptr);
//        unsafe { allocator.freelist.push_node(ptr) };
//    }
//
//    // Safe to return since we're keeping the nodes alive in the test
//    allocator
//}
//
//#[test]
//fn test_utility_functions() {
//    // Test index_to_bucket_size
//    assert_eq!(BuddyAllocator::index_to_bucket_size(0), BASIC_PAGE_SIZE);
//    assert_eq!(BuddyAllocator::index_to_bucket_size(1), BASIC_PAGE_SIZE * 2);
//    assert_eq!(BuddyAllocator::index_to_bucket_size(2), BASIC_PAGE_SIZE * 4);
//    assert_eq!(BuddyAllocator::index_to_bucket_size(3), BASIC_PAGE_SIZE * 8);
//
//    // Test page_count_to_index
//    assert_eq!(BuddyAllocator::page_count_to_index(1), 0);
//    assert_eq!(BuddyAllocator::page_count_to_index(2), 1);
//    assert_eq!(BuddyAllocator::page_count_to_index(4), 2);
//
//    // Test get_buddy_addr
//    let addr1 = PhysAddr(0x1000);
//    let addr2 = PhysAddr(0x2000);
//
//    assert_eq!(BuddyAllocator::get_buddy_addr(addr1, 0), PhysAddr(addr1.0 + BASIC_PAGE_SIZE));
//    assert_eq!(BuddyAllocator::get_buddy_addr(addr2, 0), PhysAddr(addr2.0 - BASIC_PAGE_SIZE));
//
//    // Test determine_next_zone_bucket_addr
//    assert_eq!(
//        BuddyAllocator::determine_next_zone_bucket_addr(addr1, addr2),
//        addr1
//    );
//    assert_eq!(
//        BuddyAllocator::determine_next_zone_bucket_addr(addr2, addr1),
//        addr1
//    );
//}
//
//#[test]
//fn test_mass_allocations_and_frees() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Addresses to keep track of allocations
//    let mut allocated_addresses = Vec::new();
//
//    // Allocate blocks of different sizes and alignments
//    for i in 1..=10 {
//        // Allocate with different page counts
//        let page_count = i;
//        let alignment = if i % 2 == 0 { 2 } else { 1 };
//
//        match allocator.alloc_any(alignment, page_count) {
//            Ok(addr) => {
//                allocated_addresses.push((addr, page_count));
//                println!("Allocated {} pages at {:#x} with alignment {}",
//                         page_count, addr.0, alignment);
//            },
//            Err(e) => {
//                panic!("Failed to allocate {} pages with alignment {}: {:?}",
//                       page_count, alignment, e);
//            }
//        }
//    }
//
//    // Mix allocations and frees
//    let first_addr = allocated_addresses.remove(0);
//    unsafe {
//        allocator.free(first_addr.0, first_addr.1).expect("Failed to free address");
//    }
//    println!("Freed {} pages at {:#x}", first_addr.1, first_addr.0);
//
//    // Allocate more memory
//    for i in 1..=5 {
//        let page_count = i * 2;
//        let alignment = 4;
//
//        match allocator.alloc_any(alignment, page_count) {
//            Ok(addr) => {
//                allocated_addresses.push((addr, page_count));
//                println!("Allocated {} pages at {:#x} with alignment {}",
//                         page_count, addr.0, alignment);
//            },
//            Err(e) => {
//                panic!("Failed to allocate {} pages with alignment {}: {:?}",
//                       page_count, alignment, e);
//            }
//        }
//    }
//
//    // Free everything
//    for (addr, page_count) in allocated_addresses {
//        unsafe {
//            match allocator.free(addr, page_count) {
//                Ok(_) => println!("Freed {} pages at {:#x}", page_count, addr.0),
//                Err(e) => panic!("Failed to free {} pages at {:#x}: {:?}",
//                                 page_count, addr.0, e),
//            }
//        }
//    }
//}
//
//#[test]
//fn test_error_conditions() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Test zero page allocation
//    match allocator.alloc_any(1, 0) {
//        Err(PmmError::NoAvailableBlock) => println!("Correctly rejected zero page allocation"),
//        Ok(_) => panic!("Should not allow zero page allocation"),
//        Err(e) => panic!("Unexpected error: {:?}", e),
//    }
//
//    // Test zero alignment
//    match allocator.alloc_any(0, 1) {
//        Err(PmmError::InvalidAlignment) => println!("Correctly rejected zero alignment"),
//        Ok(_) => panic!("Should not allow zero alignment"),
//        Err(e) => panic!("Unexpected error: {:?}", e),
//    }
//
//    // Test double free
//    let allocation = allocator.alloc_any(1, 1).expect("Failed to allocate page");
//
//    unsafe {
//        // First free (should succeed)
//        allocator.free(allocation, 1).expect("Failed to free page");
//
//        // Second free (should fail)
//        match allocator.free(allocation, 1) {
//            Err(PmmError::FreeOfAlreadyFree) => println!("Correctly rejected double free"),
//            Ok(_) => panic!("Should not allow double free"),
//            Err(e) => panic!("Unexpected error: {:?}", e),
//        }
//    }
//
//    // Test misaligned allocation
//    let misaligned_addr = PhysAddr(0x1234); // Not aligned to page boundary
//    match allocator.alloc_at(misaligned_addr, 1) {
//        Err(PmmError::InvalidAlignment) => println!("Correctly rejected misaligned allocation"),
//        Ok(_) => panic!("Should not allow misaligned allocation"),
//        Err(e) => panic!("Unexpected error: {:?}", e),
//    }
//}
//
//#[test]
//fn test_coalesce() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Allocate a large block
//    let addr = allocator.alloc_any(1, 8).expect("Failed to allocate 8 pages");
//
//    // Free the large block - this should trigger coalescing
//    unsafe {
//        allocator.free(addr, 8).expect("Failed to free 8 pages");
//    }
//
//    // Check that coalescing worked by verifying we can allocate the same block again
//    match allocator.alloc_any(1, 8) {
//        Ok(new_addr) => {
//            assert_eq!(addr.0, new_addr.0, "Coalescing didn't properly combine blocks");
//            println!("Coalescing successfully combined blocks");
//        },
//        Err(e) => panic!("Failed to allocate after coalescing: {:?}", e),
//    }
//}
//
//#[test]
//fn test_disband() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Allocate a large block
//    let addr = allocator.alloc_any(1, 8).expect("Failed to allocate 8 pages");
//
//    // Free the large block so it's available
//    unsafe {
//        allocator.free(addr, 8).expect("Failed to free 8 pages");
//    }
//
//    // Now allocate a smaller block at the same address
//    // This should trigger disbanding
//    match allocator.alloc_at(addr, 2) {
//        Ok(_) => println!("Successfully allocated smaller block at specific address"),
//        Err(e) => panic!("Failed to allocate after disband: {:?}", e),
//    }
//
//    // Verify disband worked correctly by allocating another small block
//    match allocator.alloc_any(1, 2) {
//        Ok(new_addr) => {
//            // The new address should be at addr + 2*BASIC_PAGE_SIZE if disband worked
//            assert_eq!(new_addr.0, addr.0 + 2 * BASIC_PAGE_SIZE,
//                       "Disband didn't properly split blocks");
//            println!("Disband successfully split blocks");
//        },
//        Err(e) => panic!("Failed to allocate after disband: {:?}", e),
//    }
//}
//
//#[test]
//fn test_page_free_status() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Allocate a page
//    let addr = allocator.alloc_any(1, 1).expect("Failed to allocate page");
//
//    // Page should not be free
//    match allocator.is_page_free(addr) {
//        Ok(false) => println!("Correctly identified allocated page as not free"),
//        Ok(true) => panic!("Incorrectly identified allocated page as free"),
//        Err(e) => panic!("Unexpected error: {:?}", e),
//    }
//
//    // Free the page
//    unsafe {
//        allocator.free(addr, 1).expect("Failed to free page");
//    }
//
//    // Page should now be free
//    match allocator.is_page_free(addr) {
//        Ok(true) => println!("Correctly identified freed page as free"),
//        Ok(false) => panic!("Incorrectly identified freed page as not free"),
//        Err(e) => panic!("Unexpected error: {:?}", e),
//    }
//}
//
//#[test]
//fn test_allocation_stress() {
//    let mut allocator = setup_buddy_allocator();
//
//    // Track allocations
//    let mut allocations = Vec::new();
//
//    // Make a series of allocations with different patterns
//    for i in 1..=10 {
//        // Mix of page sizes
//        let page_count = match i % 3 {
//            0 => 1,
//            1 => 2,
//            _ => 4,
//        };
//
//        // Mix of alignments
//        let alignment = match i % 2 {
//            0 => 1,
//            _ => 2,
//        };
//
//        if let Ok(addr) = allocator.alloc_any(alignment, page_count) {
//            allocations.push((addr, page_count));
//        }
//    }
//
//    // Free half the allocations (evens)
//    for i in (0..allocations.len()).filter(|i| i % 2 == 0) {
//        let (addr, page_count) = allocations[i];
//        unsafe {
//            allocator.free(addr, page_count).expect("Failed to free in stress test");
//        }
//        // Mark as freed by setting page_count to 0
//        allocations[i].1 = 0;
//    }
//
//    // Make a new batch of allocations
//    for i in 1..=5 {
//        let page_count = i;
//        if let Ok(addr) = allocator.alloc_any(1, page_count) {
//            allocations.push((addr, page_count));
//        }
//    }
//
//    // Free all remaining allocations
//    for (addr, page_count) in allocations.iter().filter(|(_, pages)| *pages > 0) {
//        unsafe {
//            allocator.free(*addr, *page_count).expect("Failed to free in final cleanup");
//        }
//    }
//}
