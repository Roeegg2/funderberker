//! Everything needed to boot the kernel with Limine.

use core::arch::asm;
use core::num::NonZero;

use limine::request::RsdpRequest;
use limine::BaseRevision;
use limine::memory_map;
use limine::paging;
use limine::request::{
    HhdmRequest, KernelAddressRequest, MemoryMapRequest, PagingModeRequest, RequestsEndMarker,
    RequestsStartMarker,
};

#[cfg(feature = "framebuffer")]
use limine::request::FramebufferRequest;

use crate::arch::BASIC_PAGE_SIZE;
use crate::arch::x86_64;
use crate::mem::{PhysAddr, VirtAddr, pmm};
use crate::print;
use crate::println;

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with #[used], otherwise they may be removed by the compiler.
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
    {
        #[allow(static_mut_refs)]
        unsafe {
            print::serial::SERIAL_WRITER.init().unwrap()
        };
    }
    #[cfg(feature = "framebuffer")]
    {
        let framebuffer_reponse = FRAMEBUFFER_REQUEST
            .get_response()
            .expect("Can't get Limine framebuffer feature response");
        #[allow(static_mut_refs)]
        unsafe {
            print::framebuffer::FRAMEBUFFER_WRITER
                .init_from_limine(framebuffer_reponse.framebuffers().next().unwrap())
        };
    }

    unsafe { crate::arch::init() };

    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("Can't get Limine framebuffer feature response");
    #[allow(static_mut_refs)]
    unsafe {
        crate::mem::HHDM_OFFSET = hhdm.offset() as usize;
    };

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

    unsafe { pmm::init_from_limine(mem_map.entries()) };
    unsafe {
        x86_64::paging::init_from_limine(
            mem_map.entries(),
            VirtAddr(kernel_addr.virtual_base() as usize),
            PhysAddr(kernel_addr.physical_base() as usize),
        )
        .unwrap()
    };

    unsafe {
        // Limine hands us a pointer to the RSDP that is already HHDM mapped
        crate::acpi::init(rsdp.address()).unwrap();
    };

    crate::funderberker_main();

    hcf();
}

#[panic_handler]
pub fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    use crate::println;

    println!("{}", info);
    hcf();
}

fn hcf() -> ! {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("hlt");
        asm!("hlt");
        asm!("hlt");
        asm!("hlt");
        asm!("hlt");
        asm!("hlt");
        asm!("hlt");
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        asm!("wfi");
    }

    unreachable!();
}
