use core::arch::asm;

//use limine::memory_map::EntryType;
use limine::paging;
use limine::request::{
    MemoryMapRequest, PagingModeRequest, RequestsEndMarker, RequestsStartMarker,
};
use limine::{BaseRevision, memory_map};
//use limine::{paging, response::PagingModeResponse};

#[cfg(feature = "framebuffer")]
use limine::request::FramebufferRequest;

use crate::{funderberker_main, print, println};

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with #[used], otherwise they may be removed by the compiler.
#[used]
// The .requests section allows limine to find the requests faster and more safely.
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))] // x86_64 and AArch64 share the same modes
#[used]
#[unsafe(link_section = ".requests")]
#[cfg(feature = "paging_4")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new().with_mode(paging::Mode::FOUR_LEVEL);
#[cfg(feature = "paging_5")]
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
        if let Some(framebuffer_reponse) = FRAMEBUFFER_REQUEST.get_response() {
            #[allow(static_mut_refs)]
            unsafe {
                print::framebuffer::FRAMEBUFFER_WRITER
                    .init(framebuffer_reponse.framebuffers().next().unwrap())
            };
        } else {
            // TODO:
            // Can't log
        }
    }

    if let Some(mem_map) = MEMORY_MAP_REQUEST.get_response() {
        init_pmm(mem_map.entries());
    }

    funderberker_main();

    hcf();
}

/// Initilize the PMM
fn init_pmm(mem_map: &[&memory_map::Entry]) {
    #[cfg(feature = "pmm_bump")]
    {
        let mut page_count: u64 = 0;
        mem_map
            .iter()
            .for_each(|&entry| page_count += entry.length / 4096);
        let bitmap_size = (page_count / 8).next_multiple_of(8);
        let bitmap_entry = mem_map
            .iter()
            .find(|&entry| entry.length >= bitmap_size)
            .expect("Couldn't find memory area to allocate bitmap!");

        println!("BITMAP PHYS ADDR {}", bitmap_entry.base);
        // map bitmap_addr to virt
    }
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            asm!("wfi");
            #[cfg(target_arch = "loongarch64")]
            asm!("idle 0");
        }
    }
}
