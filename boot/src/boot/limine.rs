//! Everything needed to boot the kernel with Limine.

use kernel::mem::{paging::PagingManager, vaa::init_vaa_from_limine};

use crate::{acpi, funderberker_start};
use kernel::arch::Arch;
use kernel::arch::x86_64::X86_64;
use utils::mem::{HHDM_OFFSET, PhysAddr, VirtAddr};

#[cfg(feature = "framebuffer")]
use limine::request::FramebufferRequest;
use limine::request::{
    ExecutableAddressRequest, HhdmRequest, MemoryMapRequest, PagingModeRequest, RequestsEndMarker,
    RequestsStartMarker, RsdpRequest,
};
use limine::{BaseRevision, paging};

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with `#[used]`, otherwise they may be removed by the compiler.
// The .requests section allows limine to find the requests faster and more safely.
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static KERNEL_ADDRESS_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();
#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))] // x86_64 and AArch64 share the same modes
#[used]
#[unsafe(link_section = ".requests")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new().with_mode(paging::Mode::FOUR_LEVEL);

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

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // All limine requests must also be referenced in a called function, otherwise they may be
    // removed by the linker.
    assert!(BASE_REVISION.is_supported());

    #[cfg(feature = "framebuffer")]
    logger::framebuffer::init_from_limine(
        FRAMEBUFFER_REQUEST
            .get_response()
            .unwrap()
            .framebuffers()
            .next()
            .unwrap(),
    );

    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("Can't get Limine framebuffer feature response");
    let mem_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Can't get Limine memory map feature");
    let kernel_addr = KERNEL_ADDRESS_REQUEST
        .get_response()
        .expect("Can't get Limine kernel address feature");
    let rsdp = RSDP_REQUEST
        .get_response()
        .expect("Can't get Limine RSDP feature");

    unsafe {
        HHDM_OFFSET.set(hhdm.offset() as usize);

        X86_64::early_boot_init();

        init_vaa_from_limine(mem_map.entries());

        let used_by_pmm = pmm::init_from_limine(mem_map.entries());

        X86_64::init_paging_from_limine(
            mem_map.entries(),
            VirtAddr(kernel_addr.virtual_base() as usize),
            PhysAddr(kernel_addr.physical_base() as usize),
            used_by_pmm,
        );

        acpi::init(PhysAddr(rsdp.address())).unwrap();
    };

    // XXX: As I've stated in the comment in the function below, this is technically bad since
    // there is a period of time our stack is marked as free, but during that time period nothing
    // gets allocated, so these pages will stay intact and so it shouldn't be a problem.
    // unsafe { free_bootloader_reclaimable(mem_map.entries()) };

    funderberker_start();
    // unsafe { kernel::archmigrate_to_new_stack() };
}
