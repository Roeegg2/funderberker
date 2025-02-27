use core::arch::asm;

use limine::request::{RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;

#[cfg(feature = "framebuffer")]
use limine::request::FramebufferRequest;

use crate::{funderberker_main, print};

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with #[used], otherwise they may be removed by the compiler.
#[used]
// The .requests section allows limine to find the requests faster and more safely.
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

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

    funderberker_main();

    hcf();
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
