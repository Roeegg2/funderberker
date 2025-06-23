use core::{num::NonZero, ptr::from_ref};

use logger::*;
use utils::mem::{PhysAddr, VirtAddr};

const DEFAULT_GUEST_ADDRESS_SPACE_START: GuestPhysAddr = VirtAddr(0x1000);

pub(super) struct GuestVirtAddr(pub usize);

pub(super) type GuestPhysAddr = VirtAddr;

/// Creates a new address space for a vessel.
///
/// Returns the guest physical address of the PML and a reference to PML.
pub(super) fn create_guest_address_space<'a>(
    page_count: usize,
) -> (GuestPhysAddr, &'a mut PageTable) {
    let addr_space_pml = PageTable::new().0;

    // TODO: maybe map 2mb/1gb pages in some cases
    for i in 0..page_count {
        let guest_addr = DEFAULT_GUEST_ADDRESS_SPACE_START + (i * BASIC_PAGE_SIZE);
        let page = pmm::get()
            .allocate(NonZero::new(1).unwrap(), NonZero::new(page_count).unwrap())
            .unwrap();
        unsafe {
            addr_space_pml.map(guest_addr, page, PageSize::Size4KB, Entry::FLAG_RW);
        }
    }

    (DEFAULT_GUEST_ADDRESS_SPACE_START, addr_space_pml)

    // let addr_space_pml = new_page_table();
    //
    // for i in 0..page_count {
    //      let guest_addr = DEFAULT_GUEST_ADDRESS_SPACE_START + i * BASIC_PAGE_SIZE;
    //      map guest_addr to some physical address
    // }
}

/// Creates a new page table for the given address space
///
/// Returns the guest physical address of the new page table PML as well as a reference to it
pub(super) fn new_guest_page_table(
    addr_space_pml: &mut PageTable,
) -> (GuestPhysAddr, &mut PageTable) {
    let pml = PageTable::new().0;

    let addr = VirtAddr(from_ref(pml).addr());
    unsafe {
        addr_space_pml.map(addr, PhysAddr(0x1000), PageSize::Size4KB, Entry::FLAG_RW);
    };

    (addr, pml)
    // let pml = PageTable::new();
    //
    // map the pml to the last guest_phys_addr passed in the address space
}
