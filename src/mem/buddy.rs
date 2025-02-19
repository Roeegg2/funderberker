use crate::uefi::{MemoryDescriptor, MemoryType};
use core::{ffi::c_void, ptr::NonNull};

// max of 36
const MAX_LEVELS: u8 = 36;

// TODO: Use mutex to access BUDDY_MASTER?
//static BUDDY_MASTER: UnsafeCell<BuddyMaster> = UnsafeCell::new(BuddyMaster {
//    buddies: &mut[],
//    bitmaps: &mut[],
//});
static mut BUDDY_MASTER: BuddyMaster = BuddyMaster {
    buddies: &mut [],
    bitmap: &mut [],
};

pub(super) enum BuddyError {
    ImpossibleLevelNum,
    BadBitmapSize,
}

pub(super) struct BuddyMaster {
    buddies: &'static mut [Option<u8>],
    bitmap: &'static mut [u64],
}

impl BuddyMaster {
    pub(super) fn allocate_page_block(
        block_size: u32,
    ) -> Result<NonNull<*const c_void>, BuddyError> {
        Err(BuddyError::ImpossibleLevelNum)
    }

    pub(super) fn free_page_block(page_block: *const c_void) -> Result<(), BuddyError> {
        Ok(())
    }

    pub(super) unsafe fn new(
        bitmap_descr: &MemoryDescriptor,
        phys_page_count: u64,
    ) -> Result<(), BuddyError> {
        unsafe {
            BUDDY_MASTER.bitmap = core::slice::from_raw_parts_mut(
                bitmap_descr.phys_addr_start as *mut u64,
                phys_page_count as usize,
            )
        };

        #[cfg(debug_assertions)]
        println!(
            "GOT BITMAP AT {:?}",
            bitmap_descr.phys_addr_start as *mut u64
        );
        Ok(())
    }
}

pub(super) fn allocate_bitmap(
    mem_map: *mut MemoryDescriptor,
    descr_count: u64,
    page_count: u64,
) -> Option<MemoryDescriptor> {
    let mut new_node: Option<MemoryDescriptor> = None;
    println!("REQUIRED PAGE SIZE: {:?}", page_count);
    for i in 0..descr_count {
        let mem_descr = unsafe { mem_map.offset(i.try_into().unwrap()).as_mut().unwrap() };

        match mem_descr.mem_type {
            MemoryType::ConventionalMemory
            | MemoryType::BootServicesCode
            | MemoryType::BootServicesData => {
                let aligned_addr = mem_descr.phys_addr_start.next_multiple_of(page_count * 4096);
                println!("aligned_addr is {:?}", aligned_addr);
                if aligned_addr > mem_descr.phys_addr_start + (mem_descr.page_count * 4096) {
                    continue;
                }

                new_node = Some(MemoryDescriptor {
                    mem_type: MemoryType::LoaderData,
                    phys_addr_start: aligned_addr,
                    virt_addr_start: 0,
                    page_count: page_count,
                    attr: 15, // TODO: check if you really need to set
                    _reserved: 0,
                });

                break;
                //mem_descr.phys_addr_start += (mem_descr.page_count * 4096);
            }
            _ => (),
        }
    }

    new_node
}
