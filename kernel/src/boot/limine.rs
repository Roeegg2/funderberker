//! Everything needed to boot the kernel with Limine.

use core::num::NonZero;

use limine::BaseRevision;
use limine::memory_map;
use limine::memory_map::EntryType;
use limine::paging;
use limine::request::RsdpRequest;
use limine::request::{
    HhdmRequest, KernelAddressRequest, MemoryMapRequest, PagingModeRequest, RequestsEndMarker,
    RequestsStartMarker,
};

#[cfg(feature = "framebuffer")]
use limine::request::FramebufferRequest;

use crate::arch::{self, BASIC_PAGE_SIZE, x86_64};
#[cfg(feature = "framebuffer")]
use crate::dev::framebuffer;
use crate::dev::serial;
use crate::mem::pmm::PmmAllocator;
use crate::mem::vmm;
use crate::mem::{PhysAddr, VirtAddr, pmm};

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with `#[used]`, otherwise they may be removed by the compiler.
// The .requests section allows limine to find the requests faster and more safely.
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static KERNEL_ADDRESS_REQUEST: KernelAddressRequest = KernelAddressRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();
#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[cfg(feature = "paging_4")]
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))] // x86_64 and AArch64 share the same modes
#[used]
#[unsafe(link_section = ".requests")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new().with_mode(paging::Mode::FOUR_LEVEL);
#[cfg(feature = "paging_5")]
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))] // x86_64 and AArch64 share the same modes
#[used]
#[unsafe(link_section = ".requests")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new().with_mode(paging::Mode::FIVE_LEVEL);

#[cfg(feature = "framebuffer")]
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

/// Define the stand and end markers for Limine requests.
#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

/// Get the maximum addressable page count from the memory map.
/// This is done by finding the last memory map entry that is usable and calculating the page count
pub fn get_page_count_from_mem_map(mem_map: &[&memory_map::Entry]) -> NonZero<usize> {
    let last_descr = mem_map
        .iter()
        .rev()
        .find(|&entry| match entry.entry_type {
            limine::memory_map::EntryType::USABLE
            | limine::memory_map::EntryType::BOOTLOADER_RECLAIMABLE
            | limine::memory_map::EntryType::ACPI_RECLAIMABLE
            | limine::memory_map::EntryType::KERNEL_AND_MODULES => true,
            _ => false,
        })
        .unwrap();

    NonZero::new((last_descr.base + last_descr.length) as usize / BASIC_PAGE_SIZE).unwrap()
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // All limine requests must also be referenced in a called function, otherwise they may be
    // removed by the linker.
    assert!(BASE_REVISION.is_supported());

    #[cfg(feature = "serial")]
    #[allow(static_mut_refs)]
    unsafe {
        serial::SERIAL_WRITER.init().unwrap()
    };
    #[cfg(feature = "framebuffer")]
    {
        let framebuffer_reponse = FRAMEBUFFER_REQUEST
            .get_response()
            .expect("Can't get Limine framebuffer feature response");
        #[allow(static_mut_refs)]
        unsafe {
            framebuffer::FRAMEBUFFER_WRITER
                .init_from_limine(framebuffer_reponse.framebuffers().next().unwrap())
        };
    }

    unsafe { arch::init() };

    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("Can't get Limine framebuffer feature response");

    unsafe {
        #[allow(static_mut_refs)]
        crate::mem::HHDM_OFFSET.set(hhdm.offset() as usize).unwrap();
    }

    let mem_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Can't get Limine memory map feature");
    let kernel_addr = KERNEL_ADDRESS_REQUEST
        .get_response()
        .expect("Can't get Limine kernel address feature");
    let paging_mode = PAGING_MODE_REQUEST
        .get_response()
        .expect("Can't get Limine paging mode feature");
    let rsdp = RSDP_REQUEST
        .get_response()
        .expect("Can't get Limine RSDP feature");

    match paging_mode.mode() {
        #[cfg(feature = "paging_5")]
        limine::paging::Mode::FOUR_LEVEL => {
            panic!("Got 5 level paging even though 4 was requested");
        }
        #[cfg(feature = "paging_4")]
        limine::paging::Mode::FIVE_LEVEL => {
            panic!("Got 4 level paging even though 5 was requested");
        }
        _ => (),
    }

    unsafe { vmm::init_from_limine(mem_map.entries()) };
    unsafe { pmm::init_from_limine(mem_map.entries()) };
    unsafe {
        x86_64::paging::init_from_limine(
            mem_map.entries(),
            VirtAddr(kernel_addr.virtual_base() as usize),
            PhysAddr(kernel_addr.physical_base() as usize),
        )
    };

    unsafe { crate::acpi::init(rsdp.address()).expect("Failed to initialize ACPI") };

    // XXX: As I've stated in the comment in the function below, this is technically bad since
    // there is a period of time our stack is marked as free, but during that time period nothing
    // gets allocated, so these pages will stay intact and so it shouldn't be a problem.
    // unsafe { free_bootloader_reclaimable(mem_map.entries()) };
    
    // unsafe { arch::migrate_to_new_stack() };

    crate::funderberker_main();
}

/// Free all `BOOTLOADER_RECLAIMABLE`` memory regions
///
/// NOTE: We need to make sure we call this only after we transitioned to our own paging AND setup
/// the stack for the BSP. Otherwise, this will lead to us to freeing memory that we are still
/// using.
pub unsafe extern "cdecl" fn free_bootloader_reclaimable(mem_map: &[&memory_map::Entry]) {
    // XXX: I think doing it this way is OK since I doubt anything will get allocated while doing
    // this, but it's still a bit sketchy.
    let pmm = pmm::get();
    for entry in mem_map {
        if entry.entry_type == EntryType::BOOTLOADER_RECLAIMABLE {
            let page_count = entry.length as usize / BASIC_PAGE_SIZE;

            // NOTE: We are doing this in a loop, since the buddy allocator only frees pages in blocks,
            // and this might not be aligned.
            // This code gets executed once during boot, so it's not worth optimizing
            for i in 0..page_count {
                unsafe {
                    pmm.free(
                        PhysAddr(entry.base as usize + (i * BASIC_PAGE_SIZE)),
                        NonZero::new_unchecked(1),
                    )
                    .unwrap();
                }
            }
        }
    }
}
