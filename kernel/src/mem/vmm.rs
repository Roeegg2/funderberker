//! Virtual memory manager for handing out virtual pages

use utils::sanity_assert;

use crate::{
    arch::{
        BASIC_PAGE_SIZE,
        x86_64::paging::{self, PageSize},
    },
    mem::HHDM_OFFSET,
    sync::spinlock::{SpinLock, SpinLockDropable},
};

use super::{PhysAddr, VirtAddr};
#[cfg(feature = "limine")]
use limine::memory_map;

static VIRTUAL_ADDRESS_ALLOCATOR: SpinLock<VirtualAddressAllocator> =
    SpinLock::new(VirtualAddressAllocator { next: VirtAddr(42) });

/// A simple bump page ID allocator
struct VirtualAddressAllocator {
    /// The next free ID to allocate
    next: VirtAddr,
}

impl VirtualAddressAllocator {
    /// Allocate `count` virtually contiguous block of 4KB pages
    fn bump(&mut self, count: usize) -> VirtAddr {
        let ret = self.next;
        self.next = self.next + (count * BASIC_PAGE_SIZE);
        sanity_assert!(
            self.next.0
                < unsafe {
                    #[allow(static_mut_refs)]
                    *(HHDM_OFFSET.get().unwrap())
                }
        );

        ret
    }
}

// XXX: This is a bit hacky, but it works
#[cfg(feature = "limine")]
pub fn init_from_limine(mem_map: &[&memory_map::Entry]) {
    const MIN_MEM_SPAN: usize = 8 * 0x1000 * 0x1000 * 0x1000 * 0x1000; // 8TB
    let last_entry = mem_map.last().unwrap();

    let addr = VirtAddr(last_entry.base as usize + last_entry.length as usize + BASIC_PAGE_SIZE);
    assert!((unsafe {#[allow(static_mut_refs)]
        HHDM_OFFSET.get().unwrap()} - addr.0) >= MIN_MEM_SPAN, 
        "Cannot find enough virtual memory space");

    let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();

    vaa.next = addr;

    log_info!(
        "Page ID allocator initialized with start bump address of {:?}", vaa.next);

    // for entry in mem_map {
    //     println!(
    //         "Memory map entry: {:#x} - {:#x} ({:?})",
    //         entry.base,
    //         entry.base + entry.length,
    //         match entry.entry_type {
    //             memory_map::EntryType::USABLE => "Usable",
    //             memory_map::EntryType::RESERVED => "Reserved",
    //             memory_map::EntryType::ACPI_RECLAIMABLE => "ACPI Reclaimable",
    //             memory_map::EntryType::ACPI_NVS => "ACPI NVS",
    //             memory_map::EntryType::BAD_MEMORY => "Bad Memory",
    //             memory_map::EntryType::BOOTLOADER_RECLAIMABLE => {
    //                 "Bootloader Reclaimable"
    //             }
    //             memory_map::EntryType::KERNEL_AND_MODULES => "Kernel and Modules",
    //             _ => "Unknown",
    //         }
    //     );
    // }
    // // let entry = mem_map.iter().find()
    // let mut bump = PAGE_ID_ALLOCATOR.lock();
    // // let last_entry = get_page_count_from_mem_map(mem_map);
    //
    // bump.next = 0x900000 as usize;
    //
    // println!(
    //     "Page ID allocator initialized with {:#x} pages",
    //     bump.next
    // );
}

/// Map the given physical address to some virtual address
///
/// NOTE: This doesn't allocate a page from the PMM, it just maps the given physical address to
/// some virtual address.
/// If you want to allocate a page, use `allocate_pages` instead.
pub unsafe fn map_page(phys_addr: PhysAddr, flags: usize) -> VirtAddr {
    let virt_addr = {
        let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();

        vaa.bump(1)
    };

    let pml = paging::get_pml();
    unsafe {
        pml.map(virt_addr, phys_addr, PageSize::Size4KB, flags);
    }

   virt_addr 
}

/// Allocate `count` virtually contiguous block of 4KB pages
///
/// NOTE: This function might use 2MB or 1GB pages if the allocation is large enough for it, OR if
/// it takes up less memory
#[must_use = "Not freeing the pages will cause a memory leak"]
pub fn allocate_pages(count: usize, flags: usize) -> VirtAddr {
    // Allocate the virtual addresses
    let base_virt_addr = {
        let mut vaa = VIRTUAL_ADDRESS_ALLOCATOR.lock();

        vaa.bump(count)
    };

    // TODO: Support multiple page sizes

    {
        let pml = paging::get_pml();
        pml.map_allocate(base_virt_addr, count, PageSize::Size4KB, flags);
    }

    base_virt_addr

    // perform calculation, maybe it's better to allocate a 2MB or 1GB page
    // allocate enough virtual addresses
    //
    // map_alloc x 1GB pages
    // map_alloc y 2MB pages
    // map_alloc z 4KB pages
    //
    // return virtaddr
}

/// Free a virtually contiguous block of 4KB pages
pub unsafe fn free_pages(base_addr: VirtAddr, count: usize) {
    let pml = paging::get_pml();
    unsafe {
        pml.unmap(base_addr, count, PageSize::Size4KB);
    }
}

impl SpinLockDropable for VirtualAddressAllocator {}
