use buddy::BuddyMaster;

use crate::uefi::{MemoryDescriptor, MemoryType};

mod buddy;
mod slab;

fn get_frame_count_from_uefi(mem_map: *mut MemoryDescriptor, mem_descr_count: u64) -> u64 {
    let mut total_page_count = 0;
    for i in 0..mem_descr_count {
        // accessing the UEFI mem map ptr. this is unsafe since it's a raw pointer
        let mem_descr = unsafe { mem_map.offset(i.try_into().unwrap()).as_mut().unwrap() };
        println!("MEM DESCR NO {} IS {:?}", i, mem_descr);
        match mem_descr.mem_type {
            MemoryType::ConventionalMemory
            | MemoryType::BootServicesCode
            | MemoryType::BootServicesData => total_page_count += mem_descr.page_count,
            _ => (),
        }
    }

    total_page_count
}

pub fn init(mem_map: *mut MemoryDescriptor, mem_map_size: u64, mem_descr_size: u64) {
    let mem_descr_count = mem_map_size / mem_descr_size;
    let phys_page_count = get_frame_count_from_uefi(mem_map, mem_descr_count);

    let required_page_count = (((phys_page_count * 2) / 8) / 4096).next_power_of_two();
    let bitmap_descr =
        buddy::allocate_bitmap(mem_map, mem_descr_count, required_page_count).unwrap();

    unsafe {
        BuddyMaster::new(&bitmap_descr, phys_page_count);
    };
}

///// Parse and convert the UEFI mem map to our mem bitmap map
//fn allocate_buddy_bitmap(
//    mem_map: *mut MemoryDescriptor,
//    mem_map_size: usize,
//    mem_descr_size: usize,
//) {
//    let required_pages = (((get_frame_count_from_uefi(mem_map, mem_map_size, mem_descr_size) * 2) / 8) * 4096).next_power_of_two();
//
//    let mut new_node: Option<MemoryDescriptor> = None;
//    for i in 0..mem_descr_count {
//        let mem_descr = unsafe { mem_map.offset(i.try_into().unwrap()).as_mut().unwrap() };
//        if required_pages > mem_descr.page_count || mem_descr.phys_addr_start % (required_pages * 4096) != 0 {
//            continue;
//        }
//        match mem_descr.mem_type {
//            MemoryType::ConventionalMemory
//            | MemoryType::BootServicesCode
//            | MemoryType::BootServicesData => {
//                let new_addr = mem_descr.phys_addr_start % (required_pages * 4096);
//                if new_addr > mem_descr.phys_addr_start + (mem_descr.page_count * 4096) {
//                    continue;
//                }
//
//                new_node = Some(MemoryDescriptor {
//                    mem_type: MemoryType::LoaderData,
//                    phys_addr_start: new_addr,
//                    virt_addr_start: 0,
//                    page_count: required_pages,
//                    attr: 15, // TODO: check if you really need to set
//                    _reserved: 0,
//                });
//
//                break;
//                //mem_descr.phys_addr_start += (mem_descr.page_count * 4096);
//            },
//            _ => (),
//        }
//    }
//
//    new_node?;
//    unsafe {buddy::init_buddy_master(mem_map, new_node)};
//    // now that we have the page count, we can calculate how much space buddy allocation bitmaps
//    // we will need: total_page_count * 2 bits, so `(total_page_count * 2) / 8`. then, convert to pages (since we allocate memory in pages) so divide by 4096.
//    // we then get next power of 2, since we want to keep page allocation power-of-2 aligned for
//    // buddy allocator
//    //let required_pages = (((total_page_count * 2) / 8) * 4096).next_power_of_two();
//    //let mut new_node: Option<MemoryDescriptor> = None;
//    //let mut curr_page_num = 0;
//    //let mut total_page_num = 0;
//    //let mut i = 0;
//    //loop {
//    //    // get current node
//    //    let mem_descr = unsafe { mem_map.offset(i.try_into().unwrap()).as_mut().unwrap() };
//    //    match mem_descr.mem_type {
//    //        MemoryType::ConventionalMemory
//    //        | MemoryType::BootServicesCode
//    //        | MemoryType::BootServicesData => curr_page_num += mem_descr.page_count,
//    //        _ => {
//    //            curr_page_num = 0;
//    //        },
//    //    }
//    //    if curr_page_num + mem_descr.page_count >= required_pages {
//    //        // allocate page!
//    //        // mark all the prev mem descrs as taken
//    //        for j in i_start..i {
//    //            let curr_mem_descr =
//    //                unsafe { mem_map.offset(j.try_into().unwrap()).as_mut().unwrap() };
//    //            curr_mem_descr.mem_type = MemoryType::LoaderData;
//    //        }
//    //        // if this mem descr contains EXACTLY the amount of pages left needed
//    //        let needed_pages = required_pages - curr_page_num;
//    //        if mem_descr.page_count == needed_pages {
//    //            new_node = Some(MemoryDescriptor {
//    //                mem_type: MemoryType::LoaderData,
//    //                phys_addr_start: (i_start * 4096),
//    //                virt_addr_start: 0,
//    //                page_count: needed_pages,
//    //                attr: 15, // TODO: check if you really need to set
//    //                _reserved: 0,
//    //            })
//    //        }
//    //        // mod curr node
//    //        mem_descr.page_count -= required_pages - curr_page_num;
//    //        mem_descr.phys_addr_start += mem_descr.phys_addr_start + (needed_pages * 4096);
//    //    }
//    //    curr_page_num += mem_descr.page_count;
//    //    i += 1;
//    //}
//
//
//
//
//    // get the curr_page_num 2, 4, 8, etc up until to the closest pow of 2 of `total_page_count`. we calculate the sum of
//    //let buddy_tracker = crate::util::arithmetic_sum(total_page_count.next_power_of_two().ilog2() as isize, total_page_count.next_power_of_two() as isize, 2, crate::util::Flags::AIntDRational);
//
//    //let segregated_list = [*const u64; total_page_count.next_power_of_two().ilog2()];
//
//    //println!("This is {:?}", buddy_tracker);
//
//    //assert_eq!(size_of::<MemoryDescriptor>(), mem_descr_size, "memory descriptor size not matching!");
//    //let mut total: u64 = 0;
//    //
//    ////println!("mem map size is {:?}", mem_map_size);
//    ////println!("mem descr size is {:?}", mem_descr_size);
//    //let len = mem_map_size / mem_descr_size;
//    //for i in 0..len {
//    //    println!("MEM DESCR NO {} is: {:?}", i, unsafe {mem_map.offset(i.try_into().unwrap()).as_ref().unwrap()});
//    //    total += unsafe {mem_map.offset(i.try_into().unwrap()).as_ref().unwrap()}.page_count;
//    //}
//    //
//    //println!("This is the total pages {:?}", total);
//
//    // parse mem map. get physc mem size. calculate size needed for the buddy paging structures.
//    // populate them using the uefi mem map
//    // test
//    //
//}

// parse uefi mem map and print all descrs. allocate buddy allocator bitmaps + populise them then
// pass control over to buddy allocator
// implement buddy allocator
// allocate paging structures for the kernel

// implement slab allocator?
