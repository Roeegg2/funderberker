//! A buddy allocator for the PMM

use core::{cmp::min, ptr::NonNull, slice::from_raw_parts_mut};

use super::get_page_count_from_mem_map;

use crate::BASIC_PAGE_SIZE;
use alloc::boxed::Box;
#[cfg(feature = "limine")]
use limine::memory_map::EntryType;
use utils::{
    collections::stacklist::{Node, StackList},
    mem::PhysAddr,
    sync::spinlock::{SpinLock, SpinLockable},
};

use super::{PmmAllocator, PmmError};

const FREELIST_BUCKETS_SIZE: usize = 0x200000; // 2MB freelist bucket size

pub(super) static PMM: SpinLock<BuddyAllocator<'static>> = SpinLock::new(BuddyAllocator::uninit());

/// A buddy allocator for the PMM
#[derive(Debug)]
pub(super) struct BuddyAllocator<'a> {
    /// Array of the zones of the buddy allocator
    zones: &'a mut [StackList<PhysAddr>],
    /// The freelist of the buddy allocator
    freelist: StackList<PhysAddr>,
}

impl PmmAllocator for BuddyAllocator<'_> {
    fn allocate_at(&mut self, addr: PhysAddr, mut page_count: usize) -> Result<(), PmmError> {
        if page_count == 0 {
            return Err(PmmError::EmptyAllocation);
        }

        // Round up `page_count` if needed
        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        if addr.0 % (BASIC_PAGE_SIZE * page_count) != 0 {
            return Err(PmmError::InvalidAlignment);
        }

        let start_index = page_count.ilog2() as usize;
        let (used_addr, used_index) = self.find_bucket_at(addr, start_index)?;

        self.disband(addr, start_index, used_index);

        Ok(())
    }

    fn allocate(&mut self, alignment: usize, mut page_count: usize) -> Result<PhysAddr, PmmError> {
        if page_count == 0 {
            return Err(PmmError::EmptyAllocation);
        }

        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        let start_index = page_count.ilog2() as usize;

        let (used_addr, used_index) = self.find_bucket(alignment, start_index)?;

        self.disband(used_addr, start_index, used_index);

        Ok(used_addr)
    }

    fn is_page_free(&self, addr: PhysAddr, mut page_count: usize) -> Result<bool, PmmError> {
        if page_count == 0 {
            return Err(PmmError::EmptyAllocation);
        }

        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        if addr.0 % (BASIC_PAGE_SIZE * page_count) != 0 {
            return Err(PmmError::InvalidAlignment);
        }

        let start_index = page_count.ilog2() as usize;
        for i in start_index..self.zones.len() {
            let bucket_size = 2_usize.pow(i as u32) * BASIC_PAGE_SIZE;
            for &bucket in self.zones[i].iter() {
                if bucket <= addr && addr < PhysAddr(bucket.0 + bucket_size) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    unsafe fn free(&mut self, addr: PhysAddr, mut page_count: usize) -> Result<(), PmmError> {
        if page_count == 0 {
            return Err(PmmError::EmptyFree);
        }

        page_count = page_count
            .checked_next_power_of_two()
            .ok_or(PmmError::NoAvailableBlock)?;

        if self.is_page_free(addr, page_count)? {
            return Err(PmmError::FreeOfAlreadyFree);
        }

        let zone_index = page_count.ilog2() as usize;

        self.coalesce(addr, zone_index);

        Ok(())
    }

    #[cfg(feature = "limine")]
    unsafe fn init_from_limine<'a>(
        mem_map: &'a [&'a limine::memory_map::Entry],
    ) -> &'a limine::memory_map::Entry {
        let (new_pmm, entry, page_count) =
            BuddyAllocator::new_from_limine(mem_map);
        let mut pmm = PMM.lock();
        *pmm = new_pmm;

        // Mark all free memory as free
        for entry in mem_map {
            if entry.entry_type == EntryType::USABLE {
                let page_count = entry.length as usize / BASIC_PAGE_SIZE;
                let addr = PhysAddr(entry.base as usize);

                pmm.break_into_buckets_n_free(addr, page_count);
            }
        }

        for i in 0..page_count {
            let addr = PhysAddr(entry.base as usize + i * BASIC_PAGE_SIZE);

            pmm.allocate_at(addr, 1)
                .expect("Failed to allocate a page for the buddy allocator");
        }

        entry
    }
}

impl BuddyAllocator<'_> {
    /// The lowest possible zone level (the zone level of `BASIC_PAGE_SIZE`)
    const MIN_ZONE_LEVEL: usize = BASIC_PAGE_SIZE.ilog2() as usize;

    pub(super) const fn uninit() -> Self {
        Self {
            zones: &mut [],
            freelist: StackList::new(),
        }
    }

    /// Tries to find a zone bucket that satisfies the passed `alignment` page alignment, starting
    /// from the `min_zone_index` zone index.
    ///
    /// Returns the physical address of the found zone bucket
    /// and the index of the zone
    fn find_bucket(
        &mut self,
        alignment: usize,
        start_index: usize,
    ) -> Result<(PhysAddr, usize), PmmError> {
        for i in start_index..self.zones.len() {
            // Try finding a node that satisfies the page wise alignment
            if let Some(node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data.0 % (BASIC_PAGE_SIZE * alignment) == 0)
            {
                // Save the nodes address so we can return it
                let ret = node.1.data;
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
    fn find_bucket_at(&mut self, addr: PhysAddr, start_index: usize) -> Result<(PhysAddr, usize), PmmError> {
        // Try finding a node that contains the passed `addr`
        for i in start_index..self.zones.len() {
            let bucket_size = 2_usize.pow(i as u32) * BASIC_PAGE_SIZE;
            if let Some(node) = self.zones[i]
                .iter_node()
                .enumerate()
                .find(|&node| node.1.data <= addr && addr < PhysAddr(node.1.data.0 + bucket_size))
            {
                // Save the nodes address so we can return it
                let ret = node.1.data;
                self.pop_from_zone(i, node.0);

                return Ok((ret, i));
            }
        }

        Err(PmmError::NoAvailableBlock)
    }

    /// Splits the passed `addr` into the freelist, starting from the `bucket_index` and going
    /// down (i.e. The opposite of coalescing)
    fn disband(&mut self, mut addr: PhysAddr, start_index: usize, used_index: usize) {
        // For each zone under the used index, we push a new node
        for i in start_index..used_index {
            let buddy_addr = Self::get_buddy_addr(addr, i);
            self.push_to_zone(buddy_addr, i);
            addr = Self::determine_next_bucket_addr(addr, buddy_addr);
        }
    }

    /// Coalesces the passed `addr` into the freelist, starting from the `min_zone_index` and going
    /// up
    fn coalesce(&mut self, mut addr: PhysAddr, start_index: usize) {
        for i in start_index..self.zones.len() {
            let buddy_addr = Self::get_buddy_addr(addr, i);
            if let Some(buddy_node) = self.zones[i].iter_node().enumerate().find(|&node| node.1.data == buddy_addr) {
                // If the buddy is here, then we can coalesce. Logically this means combining the
                // two to a node in the next zone level.
                // What we do is just remove the buddy from the zone, and then after we finished
                // coalescing with each level, we just push a node to the final level
                self.pop_from_zone(i, buddy_node.0);
                addr = Self::determine_next_bucket_addr(addr, buddy_addr);
            } else {
                // If the buddy isn't here then we can't coalesce anymore, so we're done popping
                // nodes and we can push the node
                self.push_to_zone(addr, i);
                return;
            }
        }

        unreachable!();
    }

    /// Returns the buddy address of the passed `addr` in the passed `zone_index`
    ///
    /// NOTE: This method assumes that the passed `addr` belongs to the passed `zone_index`. An
    /// invalid buddy address will be returned if this is not the case.
    #[inline]
    fn get_buddy_addr(addr: PhysAddr, zone_index: usize) -> PhysAddr {
        let bucket_size = 2_usize.pow((zone_index + Self::MIN_ZONE_LEVEL) as u32);
        assert!(addr.0 % bucket_size == 0);

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
    fn determine_next_bucket_addr(addr: PhysAddr, buddy_addr: PhysAddr) -> PhysAddr {
        min(addr, buddy_addr)
    }

    /// Pops a node from the freelist and pushes it to the zone at the passed `zone_index` and
    /// `buddy_index`
    #[inline]
    fn pop_from_zone(&mut self, zone_index: usize, node_index: usize) {
        let node = Box::into_non_null(self.zones[zone_index].remove_at(node_index).unwrap());

        unsafe { self.freelist.push_node(node) };
    }

    /// Pushes the passed `buddy_addr` to the zone at the passed `zone_index`
    fn push_to_zone(&mut self, buddy_addr: PhysAddr, zone_index: usize) {
        // Move the node from the freelist to `zones[zone_index]`
        let mut buddy = self.freelist.pop_node().unwrap();
        buddy.data = buddy_addr;
        unsafe {
            self.zones[zone_index].push_node(Box::into_non_null(buddy));
        };
    }

    fn break_into_buckets_n_free(&mut self, addr: PhysAddr, mut total_page_count: usize) {
        let mut addr_id = addr.0 / BASIC_PAGE_SIZE;
        'outer:
        loop {
            if total_page_count == 0 {
                break;
            }

            for i in (0..self.zones.len()).rev() {
                let page_count = 2_usize.pow(i as u32);
                if addr_id % page_count == 0 && total_page_count >= page_count {
                    unsafe {
                        self.free(PhysAddr(addr_id * BASIC_PAGE_SIZE), page_count).unwrap();
                    };
                    total_page_count -= page_count;
                    addr_id += page_count;
                    continue 'outer;
                }
            }

            unreachable!();
        }
    }

    /// Creates a new instance of the BuddyAllocator
    /// TODO: Use the leftover memory as well
    #[cfg(feature = "limine")]
    pub fn new_from_limine<'a>(
        mem_map: &'a [&'a limine::memory_map::Entry],
    ) -> (Self, &'a limine::memory_map::Entry, usize) {
        let zones_count = {
            let total_page_count = get_page_count_from_mem_map(mem_map);

            total_page_count.ilog2() as usize + 1
        };

        let total_buffer_size = {
            let zones_size = zones_count * size_of::<StackList<PhysAddr>>();
            let align_to_add = zones_size % align_of::<Node<PhysAddr>>();

            zones_size + align_to_add + FREELIST_BUCKETS_SIZE
        };

        // Find a matching entry in Limine's memory map
        let entry = *mem_map.iter().find(|&&entry| matches!(entry.entry_type, EntryType::USABLE if entry.length as usize >= total_buffer_size)).unwrap();

        // Create a pointer to it
        let zones_ptr =
            PhysAddr(entry.base as usize).add_hhdm_offset().0 as *mut StackList<PhysAddr>;

        let ret = Self {
            zones: Self::create_zones(zones_ptr, zones_count),
            freelist: Self::create_freelist(zones_ptr, zones_count),
        };

        // Mark the memory we used as taken
        (ret, entry, total_buffer_size.div_ceil(BASIC_PAGE_SIZE))
    }

    fn create_zones<'a>(zones_ptr: *mut StackList<PhysAddr>, zones_count: usize) -> &'a mut [StackList<PhysAddr>] {
        let zones = unsafe { from_raw_parts_mut(zones_ptr, zones_count) };

        for i in 0..zones_count {
            zones[i] = StackList::new();
        }

        zones
    }

    fn create_freelist(zones_ptr: *mut StackList<PhysAddr>, max_zone_level: usize) -> StackList<PhysAddr> {
        let mut freelist = StackList::new();
        // Push the pointer all the way to the end of the zones array, since this is the start
        // of the buffer for the freelist nodes
        let ptr = unsafe { zones_ptr.add(max_zone_level) };

        // Make sure the pointer is aligned to the size of `Node<PhysAddr>`
        let align_offset = ptr.align_offset(align_of::<Node<PhysAddr>>());
        let ptr = unsafe { ptr.byte_add(align_offset).cast::<Node<PhysAddr>>() };

        // Push the nodes to the freelist
        let buckets_count = FREELIST_BUCKETS_SIZE / size_of::<Node<PhysAddr>>();
        for i in 0..buckets_count {
            unsafe {
                freelist.push_node(NonNull::new(ptr.add(i)).unwrap());
            }
        }

        freelist
    }
}

unsafe impl Send for BuddyAllocator<'_> {}
unsafe impl Sync for BuddyAllocator<'_> {}

impl SpinLockable for BuddyAllocator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_ADDR: PhysAddr = PhysAddr(0x1000000); // 16MB base address for testing

    // Mock setup for testing
    fn new_allocator(zones_count: usize) -> BuddyAllocator<'static> {
        let zones: Box<[StackList<PhysAddr>]> = (0..zones_count)
            .map(|_| StackList::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let mut freelist = StackList::new();

        // Initialize freelist with dummy nodes
        for i in 0..400 {
            let addr = PhysAddr(i * BASIC_PAGE_SIZE);
            let node = Node::new(addr);
            unsafe {
                freelist.push_node(Box::into_non_null(Box::new(node)));
            }
        }

        BuddyAllocator {
            zones: Box::leak(zones),
            freelist,
        }
    }

    impl BuddyAllocator<'_> {
        fn setup_mem_map(&mut self, page_count: usize) {
            // Break the memory region into appropriately sized buckets and mark them as free
            self.break_into_buckets_n_free(BASE_ADDR, page_count);
        }
    }

    #[test]
    fn break_into_buckets_n_free_one_page() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(1);

        assert_eq!(allocator.zones[0].len(), 1);

        // All other zones should be empty
        for (i, zone) in allocator.zones.iter().enumerate() {
            if i != 0 {
                assert_eq!(zone.len(), 0);
            }
        }
    }

    #[test]
    fn break_into_buckets_n_free_two_pages() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(2);
        
        // 2 pages should create one 2-page bucket in zone 1
        assert_eq!(allocator.zones[1].len(), 1);
        
        // All other zones should be empty
        for (i, zone) in allocator.zones.iter().enumerate() {
            if i != 1 {
                assert_eq!(zone.len(), 0);
            }
        }
    }

    #[test]
    fn break_into_buckets_n_free_three_pages() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(3);

        // 3 pages = 2 + 1, should have one 2-page bucket and one 1-page bucket
        assert_eq!(allocator.zones[0].len(), 1);
        assert_eq!(allocator.zones[1].len(), 1);
    }

    #[test]
    fn break_into_buckets_n_free_ten_pages() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(10);

        assert_eq!(allocator.zones[1].len(), 1);
        assert_eq!(allocator.zones[3].len(), 1);
    }

    #[test]
    fn break_into_buckets_n_free_no_overlaps() {
        let mut allocator = new_allocator(33);
        let page_count = 63; // Non-power-of-two for complex splitting

        allocator.break_into_buckets_n_free(BASE_ADDR, page_count);

        // Collect all allocated ranges
        let mut ranges = Vec::new();
        for (i, zone) in allocator.zones.iter().enumerate() {
            let bucket_size = 2_usize.pow((i + BuddyAllocator::MIN_ZONE_LEVEL) as u32);
            for addr in zone.iter() {
                ranges.push((addr.0, addr.0 + bucket_size));
            }
        }

        // Sort ranges by start address
        ranges.sort_by_key(|&(start, _)| start);

        // Check no overlaps
        for window in ranges.windows(2) {
            let (_, end1) = window[0];
            let (start2, _) = window[1];
            assert!(end1 <= start2);
        }
    }

    #[test]
    fn break_into_buckets_n_free_specific_decompositions() {
        // Test specific decompositions to ensure correctness
        let test_cases = vec![
            (6, vec![(4, 1), (2, 1)]),   // 6 = 4 + 2
            (9, vec![(8, 1), (1, 1)]),   // 9 = 8 + 1
            (11, vec![(8, 1), (2, 1), (1, 1)]), // 11 = 8 + 2 + 1
            (15, vec![(8, 1), (4, 1), (2, 1), (1, 1)]), // 15 = 8 + 4 + 2 + 1
        ];

        for (page_count, expected_buckets) in test_cases {
            let mut allocator = new_allocator(33);
            allocator.setup_mem_map(page_count);

            for (bucket_pages, expected_count) in expected_buckets {
                let zone_index = (bucket_pages as usize).ilog2() as usize;
                assert_eq!(allocator.zones[zone_index].len(), expected_count);
            }
        }
    }
    
    #[test]
    fn break_into_buckets_n_free_stress_test() {
        // Test with a very large region
        let mut allocator = new_allocator(33);
        let page_count = 2047; // Large non-power-of-two

        allocator.setup_mem_map(page_count);

        // Verify total page count
        let mut total_pages = 0;
        for (i, zone) in allocator.zones.iter().enumerate() {
            let bucket_pages = 2_usize.pow(i as u32);
            total_pages += zone.len() * bucket_pages;
        }

        assert_eq!(total_pages, page_count);

        // Should be able to allocate many different sizes
        for size in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512] {
            if size < page_count {
                assert!(allocator.allocate(1, size).is_ok());
            }
        }
    }

    #[test]
    fn allocate_and_free() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(1000);

        for _ in 0..200 {
            let addr = allocator.allocate(1, 4).unwrap();
            assert!(!allocator.is_page_free(addr, 1).unwrap());

            // Free the page
            unsafe {
                allocator.free(addr, 4).unwrap();
            }

            assert!(allocator.is_page_free(addr, 4).unwrap());
        }

        // Ensure we can still allocate after freeing
        let _ = allocator.allocate(1, 1).unwrap();
    } 

    #[test]
    fn test_allocate_no_available_blocks() {
        let mut allocator = new_allocator(33);
        
        // Don't add any free blocks
        
        let result = allocator.allocate(1, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::NoAvailableBlock);
    }

    #[test]
    fn test_free_coalescing() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(1000);

        // Allocate a few pages
        let addr1 = allocator.allocate(1, 4).unwrap();
        let addr2 = allocator.allocate(1, 4).unwrap();

        // Free the first page
        unsafe {
            allocator.free(addr1, 4).unwrap();
        }

        // Now we should be able to coalesce
        assert!(allocator.is_page_free(addr1, 4).unwrap());
        assert!(!allocator.is_page_free(addr1, 8).unwrap());

        // Free the second page
        unsafe {
            allocator.free(addr2, 4).unwrap();
        }

        // Both pages should now be free and coalesced
        assert!(allocator.is_page_free(addr1, 8).unwrap());
    }

    // Stress coalescing tests
    #[test]
    fn test_stress_coalescing() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(2048);

        // Allocate many pages of different sizes
        let mut allocations = Vec::new();
        
        // Allocate various sized blocks
        for _ in 0..50 {
            if let Ok(addr) = allocator.allocate(1, 1) {
                allocations.push((addr, 1));
            }
        }
        for _ in 0..25 {
            if let Ok(addr) = allocator.allocate(1, 2) {
                allocations.push((addr, 2));
            }
        }
        for _ in 0..10 {
            if let Ok(addr) = allocator.allocate(1, 4) {
                allocations.push((addr, 4));
            }
        }

        // Free them all in random order to trigger coalescing
        while !allocations.is_empty() {
            let idx = allocations.len() / 2; // Simple deterministic "random"
            let (addr, size) = allocations.remove(idx);
            unsafe {
                allocator.free(addr, size).unwrap();
            }
        }

        // Should be able to allocate large blocks after coalescing
        assert!(allocator.allocate(1, 64).is_ok());
        assert!(allocator.allocate(1, 128).is_ok());
    }

    #[test]
    fn test_coalescing_multiple_sizes() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        // Allocate 4 pages of size 4 each (uses up 16 pages total)
        let addr1 = allocator.allocate(1, 4).unwrap();
        let addr2 = allocator.allocate(1, 4).unwrap();
        let addr3 = allocator.allocate(1, 4).unwrap();
        let addr4 = allocator.allocate(1, 4).unwrap();

        // All zones should be empty now
        for zone in allocator.zones.iter() {
            assert_eq!(zone.len(), 0);
        }

        // Free adjacent pairs to trigger coalescing
        unsafe {
            allocator.free(addr1, 4).unwrap();
            allocator.free(addr2, 4).unwrap();
        }

        // Should have coalesced into one 8-page block
        assert_eq!(allocator.zones[3].len(), 1); // 2^3 = 8 pages

        unsafe {
            allocator.free(addr3, 4).unwrap();
            allocator.free(addr4, 4).unwrap();
        }

        // Should have another 8-page block, but they are adjecent so they should coalesce
        assert_eq!(allocator.zones[3].len(), 0); // 2^3 = 8 pages
        assert_eq!(allocator.zones[4].len(), 1);

        // Now allocate a 16-page block
        let large_addr = allocator.allocate(1, 16).unwrap();

        for zone in allocator.zones.iter() {
            assert_eq!(zone.len(), 0); // All zones should be empty after allocation
        }
    }

    // Disband tests
    #[test]
    fn test_disband_creates_correct_buddies() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(32);

        // Allocate a 16-page block (zone index 4)
        let addr = allocator.allocate(1, 16).unwrap();

        // The disband should have created buddies in smaller zones
        // When we allocated 16 pages from a 32-page block, the remaining 16 pages
        // should be split and placed in appropriate zones

        // Free the allocated block
        unsafe {
            allocator.free(addr, 16).unwrap();
        }

        // After freeing, we should be able to coalesce back to the original size
        let large_addr = allocator.allocate(1, 32).unwrap();
        assert!(large_addr.0 >= BASE_ADDR.0 && large_addr.0 < BASE_ADDR.0 + 32 * BASIC_PAGE_SIZE);
    }

    #[test]
    fn test_disband_complex_splitting() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(64);

        // Allocate from the largest available block
        let addr1 = allocator.allocate(1, 1).unwrap(); // This will split a large block

        // Verify that buddies were created in appropriate zones
        let mut total_free_pages = 0;
        for (i, zone) in allocator.zones.iter().enumerate() {
            let bucket_pages = 2_usize.pow(i as u32);
            total_free_pages += zone.len() * bucket_pages;
        }

        assert_eq!(total_free_pages, 63); // 64 - 1 allocated

        // Free the page and verify full coalescing
        unsafe {
            allocator.free(addr1, 1).unwrap();
        }

        // Should be able to allocate the full 64 pages again
        assert!(allocator.allocate(1, 64).is_ok());
    }

    // allocate_at tests
    #[test]
    fn test_allocate_at_success() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(32);

        // Try to allocate at a specific address within the available range
        let target_addr = BASE_ADDR + (8 * BASIC_PAGE_SIZE); // Aligned address
        allocator.allocate_at(target_addr, 4).unwrap();

        // Verify the pages are no longer free
        assert!(!allocator.is_page_free(target_addr, 4).unwrap());
    }

    #[test]
    fn test_allocate_at_alignment_error() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(32);

        // Try to allocate at a misaligned address
        let misaligned_addr = BASE_ADDR + BASIC_PAGE_SIZE; // Not aligned for 4-page allocation
        let result = allocator.allocate_at(misaligned_addr, 4);
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::InvalidAlignment);
    }

    #[test]
    fn test_allocate_at_no_containing_block() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        // Try to allocate at an address outside the available range
        let outside_addr = PhysAddr(BASE_ADDR.0 + 32 * BASIC_PAGE_SIZE);
        let result = allocator.allocate_at(outside_addr, 1);
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::NoAvailableBlock);
    }

    #[test]
    fn test_allocate_at_already_allocated() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        // First allocation should succeed
        assert!(allocator.allocate_at(BASE_ADDR, 4).is_ok());

        // Second allocation at the same address should fail
        let result = allocator.allocate_at(BASE_ADDR, 4);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::NoAvailableBlock);
    }

    // Edge cases for allocate and free
    #[test]
    fn test_allocate_zero_pages() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        let result = allocator.allocate(1, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::EmptyAllocation);
    }

    #[test]
    fn test_allocate_at_zero_pages() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        let result = allocator.allocate_at(BASE_ADDR, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::EmptyAllocation);
    }

    #[test]
    fn test_free_zero_pages() {
        let mut allocator = new_allocator(33);
        
        let result = unsafe { allocator.free(BASE_ADDR, 0) };
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::EmptyFree);
    }

    #[test]
    fn test_free_already_free_page() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        // let addr = PhysAddr(0x1000000);
        
        // First free should fail because the page is already free
        let result = unsafe { allocator.free(BASE_ADDR, 1) };
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::FreeOfAlreadyFree);
    }

    #[test]
    fn test_is_page_free_zero_pages() {
        let allocator = new_allocator(33);
        
        let result = allocator.is_page_free(BASE_ADDR, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::EmptyAllocation);
    }

    #[test]
    fn test_is_page_free_misaligned() {
        let allocator = new_allocator(33);
        
        let misaligned_addr = PhysAddr(0x1000001); // Not page-aligned
        let result = allocator.is_page_free(misaligned_addr, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::InvalidAlignment);
    }

    #[test]
    fn test_allocate_overflow() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(16);

        // Try to allocate more pages than can fit in usize
        let result = allocator.allocate(1, usize::MAX);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PmmError::NoAvailableBlock);
    }

    #[test]
    fn test_alignment_requirements() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(64);

        // Test various alignment requirements
        let addr1 = allocator.allocate(2, 4).unwrap(); // 2-page alignment
        assert_eq!(addr1.0 % (2 * BASIC_PAGE_SIZE), 0);

        let addr2 = allocator.allocate(4, 8).unwrap(); // 4-page alignment  
        assert_eq!(addr2.0 % (4 * BASIC_PAGE_SIZE), 0);

        let addr3 = allocator.allocate(8, 1).unwrap(); // 8-page alignment
        assert_eq!(addr3.0 % (8 * BASIC_PAGE_SIZE), 0);
    }

    #[test]
    fn test_fragmentation_and_coalescing_recovery() {
        let mut allocator = new_allocator(33);
        allocator.setup_mem_map(128);

        // Create fragmentation by allocating alternating blocks
        let mut odd_allocations = Vec::new();
        let mut even_allocations = Vec::new();

        for i in 0..128 {
            if let Ok(addr) = allocator.allocate(1, 1) {
                if i % 2 == 0 {
                    even_allocations.push(addr);
                } else {
                    odd_allocations.push(addr);
                }
            }
        }

        // Free odd allocations to create fragmentation
        for addr in odd_allocations {
            unsafe {
                allocator.free(addr, 1).unwrap();
            }
        }

        // Should not be able to allocate large contiguous block due to fragmentation
        assert_eq!(allocator.allocate(1, 32), Err(PmmError::NoAvailableBlock));

        // Free even allocations to enable coalescing
        for addr in even_allocations {
            unsafe {
                allocator.free(addr, 1).unwrap();
            }
        }

        // Should now be able to allocate large blocks after coalescing
        assert!(allocator.allocate(1, 128).is_ok());
    }
}
