use core::{
    alloc::Layout,
    ptr::{NonNull, without_provenance_mut},
    slice::from_raw_parts_mut,
};

use alloc::boxed::Box;
use limine::memory_map::EntryType;
use utils::collections::stacklist::{Node, StackList};

use crate::{
    arch::BASIC_PAGE_SIZE,
    boot::limine::get_aaa,
    mem::{PageId, PhysAddr, addr_to_page_id},
};

use super::PmmAllocator;

pub static mut BUDDY_ALLOCATOR: BuddyAllocator = BuddyAllocator {
    zone_upper_bound: 0,
    zones: &mut [],
    freelist: StackList::new(),
};

/// The lower bound of the zone level
const ZONE_LOWER_BOUND: usize = BASIC_PAGE_SIZE.ilog2() as usize;

// XXX: I think one page should be enough for the initial freelist nodes. Might be wrong though :)
// TODO: Write the function to calculate the optimal page count for each new node
// allocation spree
/// The index of the zone level where we allocate new nodes
const FREELIST_ALLOC_ZONE_LEVEL_INDEX: usize = 0;

/// The number of nodes in a basic page
const NODES_PER_BASIC_PAGE: usize = BASIC_PAGE_SIZE / core::mem::size_of::<Node<PhysAddr>>();

/// Buddy allocator implementation of the PMM
pub(super) struct BuddyAllocator<'a> {
    zone_upper_bound: usize,
    zones: &'a mut [StackList<PhysAddr>],
    freelist: StackList<PhysAddr>,
}

impl<'a> PmmAllocator for BuddyAllocator<'a> {
    unsafe fn init_from_limine(mem_map: &[&limine::memory_map::Entry]) {
        fn init_zones_n_freelist(ptr: *mut StackList<PhysAddr>) {
            // First thing we allocate is the `zones` array
            unsafe {
                BUDDY_ALLOCATOR.zones = from_raw_parts_mut(
                    ptr,
                    BuddyAllocator::zone_level_to_index(BUDDY_ALLOCATOR.zone_upper_bound) + 1,
                );
            }

            // Then we allocate the initial freelist nodes
            unsafe {
                // Get the size of `zones` in bytes and add some padding if needed so it's aligned to
                // node
                let mut zone_size = core::mem::size_of_val(BUDDY_ALLOCATOR.zones);
                zone_size = zone_size + (zone_size % core::mem::align_of::<Node<PhysAddr>>());

                // Get the ptr
                let ptr: *mut Node<PhysAddr> = without_provenance_mut(ptr.addr() + zone_size);

                // Calculate the total amount of nodes we can fit
                let len = (2 * BASIC_PAGE_SIZE - zone_size)
                    / Layout::new::<Node<()>>().pad_to_align().size();

                // Push all of the nodes to the freelist
                for node in from_raw_parts_mut(ptr, len) {
                    #[allow(static_mut_refs)]
                    BUDDY_ALLOCATOR.freelist.push_node(NonNull::from_mut(node));
                }
            }
        }

        fn freelist_init_mem_map_entries(mem_map: &[&limine::memory_map::Entry]) {
            let upper_bound = unsafe {BUDDY_ALLOCATOR.zone_upper_bound};
            // Finally, we add the initial usable memory to the freelist
            mem_map.iter().for_each(|&entry| match entry.entry_type {
                EntryType::BOOTLOADER_RECLAIMABLE | EntryType::USABLE => unsafe {
                    //println!("this is entry.base {:x}", entry.base);

                    // TODO: DO THIS!

                    let addr = entry.base as usize;
                    let mut page_count = addr_to_page_id(entry.length as usize).unwrap();
                    println!("this is page_count {}", page_count);
                    if addr % BASIC_PAGE_SIZE != 0 {
                        unreachable!("Shouldn't have happened!");
                    }
                    for i in (ZONE_LOWER_BOUND..=upper_bound).rev(){
                        if page_count == 0 {
                            break;
                        }

                        println!("here!");
                        let r = addr % 2_usize.pow(i as u32);
                        let count = (page_count / i) - (r / BASIC_PAGE_SIZE);
                        println!("this is count {}", count);
                        #[allow(static_mut_refs)]
                        BUDDY_ALLOCATOR
                            .free(PhysAddr(addr + r), count)
                            .unwrap();
                        page_count -= count;
                    }

                    if page_count != 0 {
                        println!("page_count is {}", page_count);
                        println!("addr is {:x}", addr);
                        unreachable!("AHAHAH");
                    }
                },
                _ => (),
            })
        }
        // Calculate the total number of addressable pages
        // TODO: Change function name
        let addr = get_aaa(mem_map);
        unsafe {
            BUDDY_ALLOCATOR.zone_upper_bound = {
                if addr.is_power_of_two() {
                    addr.next_power_of_two().ilog2()
                } else {
                    (addr / 2).next_power_of_two().ilog2()
                }
            } as usize;
        }

        // Find an entry to allocate the `zones` segregated array and for the initial freelist
        // nodes
        {
            let ptr = {
                let freelist_entry = mem_map
                    .iter()
                    .find(|entry| match entry.entry_type {
                        // At least 2 pages, and make sure it's useable
                        EntryType::USABLE | EntryType::BOOTLOADER_RECLAIMABLE
                            if entry.length as usize >= 2 * BASIC_PAGE_SIZE =>
                        {
                            true
                        }
                        _ => false,
                    })
                    .unwrap();

                // Convert the physical address in the entry to a virtual one, then convert that to a valid
                // pointer
                PhysAddr(freelist_entry.base as usize).add_hhdm_offset().0
                    as *mut StackList<PhysAddr>
            };

            init_zones_n_freelist(ptr);
        }

        freelist_init_mem_map_entries(mem_map);

        // TODO: Mark 0 as taken, and mark the 2 pages allocated for the freelist as taken as well
    }

    // XXX: Zone level vs index to zones!!!
    #[inline]
    fn alloc_at(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let zone_level = page_count.next_power_of_two().ilog2() as usize;
        if addr.0 % 2_usize.pow(zone_level as u32) != 0 {
            return Err(super::PmmError::InvalidAlignment);
        }

        self.add_node_at(addr, zone_level, false)
    }

    #[inline]
    fn alloc_any(
        &mut self,
        alignment: PageId,
        page_count: usize,
    ) -> Result<PhysAddr, super::PmmError> {
        let zone_level = page_count.next_power_of_two().ilog2() as usize;

        self.add_node(alignment, zone_level, false)
    }

    unsafe fn free(&mut self, addr: PhysAddr, page_count: usize) -> Result<(), super::PmmError> {
        let zone_level = page_count.next_power_of_two().ilog2() as usize;
        if addr.0 % 2_usize.pow(zone_level as u32) != 0 {
            println!("this is addr.0 {:x}", addr.0);
            println!("this is zone_level {:x}", 2_usize.pow(zone_level as u32));
            return Err(super::PmmError::InvalidAlignment);
        }

        let index = Self::zone_level_to_index(zone_level);
        for node in self.zones[index].iter() {
            if *node == addr {
                return Err(super::PmmError::FreeOfAlreadyFree);
            }
        }

        // Add a node to the current zone level to mark the current address as free
        self.move_from_freelist(index, addr, false)?;

        Ok(())
    }

    fn is_page_free(&self, addr: PhysAddr) -> Result<bool, super::PmmError> {
        let upper_bound = core::cmp::min(
            self.zone_upper_bound,
            addr.0.next_power_of_two().ilog2() as usize,
        );

        for zone_level in
            Self::zone_level_to_index(ZONE_LOWER_BOUND)..=Self::zone_level_to_index(upper_bound)
        {
            for node in self.zones[zone_level].iter() {
                if *node == addr {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

impl<'a> BuddyAllocator<'a> {
    /// Convert a zone level to an index in the `zones` array
    #[inline(always)]
    const fn zone_level_to_index(zone_level: usize) -> usize {
        zone_level - ZONE_LOWER_BOUND
    }

    /// Convert an index in the `zones` array to a zone level
    #[inline(always)]
    const fn index_to_zone_level(index: usize) -> usize {
        index + ZONE_LOWER_BOUND
    }

    /// Add a node to the buddy allocator at the specified zone level
    fn add_node(
        &mut self,
        alignment: PageId,
        index: usize,
        emergency: bool,
    ) -> Result<PhysAddr, super::PmmError> {
        // If we're out of bounds, return an error
        if index >= self.zones.len() {
            return Err(super::PmmError::NoAvailableBlock);
        }

        // Find a node with a matching alignment
        if let Some(node) = self.zones[index]
            .iter_node()
            .enumerate()
            .find(|&node| node.1.data.0 % alignment == 0)
        {
            // TODO: When you get the chance, don't use remote as it's extra work for nothing - we
            // already have the node
            let node = self.zones[index].remove_at(node.0).unwrap();
            let ret = node.data;
            unsafe { self.freelist.push_node(Box::into_non_null(node)) };
            return Ok(ret);
        }

        // If we didn't find a free node at the current level, try the next one
        let addr = self.add_node(alignment, index + 1, emergency)?;
        // Whatever address we got, it's buddy on this level will hold the other half of the block
        let buddy_addr = PhysAddr(addr.0 + 2_usize.pow(Self::index_to_zone_level(index) as u32));
        // Move a node from the freelist to the zone list
        self.move_from_freelist(index, buddy_addr, emergency)?;

        Ok(addr)
    }

    /// Add a node to the buddy allocator at the specified zone level with the specified address
    fn add_node_at(
        &mut self,
        addr: PhysAddr,
        index: usize,
        emergency: bool,
    ) -> Result<(), super::PmmError> {
        // If we're out of bounds, return an error
        if index >= self.zones.len() {
            return Err(super::PmmError::NoAvailableBlock);
        }

        // TODO: alignment check

        // If we found a node at this level matching the wanted address, we remove it from the zone
        // list, and push it's node to the freelist
        if let Some(node) = self.zones[index]
            .iter_node()
            .enumerate()
            .find(|&node| node.1.data == addr)
        {
            // TODO: When you get the chance, don't use remote as it's extra work for nothing - we
            // already have the node
            let node = self.zones[index].remove_at(node.0).unwrap();
            unsafe { self.freelist.push_node(Box::into_non_null(node)) };
            return Ok(());
        }

        // If we didn't find a free node at the current level, try the next one
        self.add_node_at(addr, index + 1, emergency)?;
        // Whatever address we got, it's buddy on this level will hold the other half of the block
        let buddy_addr = PhysAddr(addr.0 + 2_usize.pow(Self::index_to_zone_level(index) as u32));
        // Move a node from the freelist to the zone list
        self.move_from_freelist(index, buddy_addr, emergency)
    }

    /// Move a node from the freelist to the zone list and initialize it
    fn move_from_freelist(
        &mut self,
        index: usize,
        data: PhysAddr,
        emergency: bool,
    ) -> Result<(), super::PmmError> {
        // If we have exactly as many free nodes as we have zones, we don't yet allocate the new
        // node as requested, but instead allocate a new page of nodes so we can allocate more.
        // This is because we'll (in the worst case) need a maximum of self.zones.len() nodes just
        // to allocate another node (this happens when we'll need to go up to the top level to
        // allocate a new node - that means we'll have to split self.zones.len() nodes).
        //
        // `emergency` is used to prevent infinite recursion. It's always set to false, except when
        // we call `add_node` from here.
        if self.freelist.len() == self.zones.len() {
            // Allocate a node for the new page of nodes
            let ptr = NonNull::new(without_provenance_mut(
                self.add_node(1, FREELIST_ALLOC_ZONE_LEVEL_INDEX, true)?
                    .add_hhdm_offset()
                    .0,
            ))
            .unwrap();

            // Add each node in the page to the freelist
            for i in 0..NODES_PER_BASIC_PAGE {
                unsafe { self.freelist.push_node(ptr.add(i)) };
            }
        } else if self.freelist.len() < self.zones.len() && !emergency {
            unreachable!("Something is really fucked up");
        }

        // Move a node from the freelist to the zone list
        let mut buddy = self.freelist.pop_node().expect("uh oh");
        // Initialize it
        buddy.data = data;
        // Push it to the zone list
        unsafe {
            self.zones[index].push_node(Box::into_non_null(buddy));
        }

        Ok(())
    }
}
