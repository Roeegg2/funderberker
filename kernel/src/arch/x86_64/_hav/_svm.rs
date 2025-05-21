//! HAV technology for AMD CPUs

use core::{arch::{naked_asm, x86_64::__cpuid}, num::NonZero};

use modular_bitfield::prelude::*;
use utils::sanity_assert;

use crate::{arch::{x86_64::{cpu::{rdmsr, wrmsr, AmdMsr}, paging::Entry}, BASIC_PAGE_SIZE}, mem::{pmm::{self, PmmAllocator}, vmm::map_page, PhysAddr}};

use super::Hav;

pub struct Svm;

#[repr(packed)]
struct Vmcb {

}

#[naked]
unsafe extern "C" fn do_vmrun() {
    unsafe {
    naked_asm!(
        ""
    );
    };
}

impl Hav for Svm {
    fn start() {
        Self::enable();

        let host_state_save_addr = pmm::get().allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap()).unwrap();

        unsafe {
            let write = ((host_state_save_addr.0 & 0xffff_ffff) as u32, ((host_state_save_addr.0 >> 32) & 0xffff_ffff) as u32);
            wrmsr(AmdMsr::VmHsavePa,  write.0, write.1);
        }
        
        let vmcb_phys_addr = pmm::get().allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap()).unwrap();

        // TODO: Make sure this is mapped as WB
        let vmcb_ptr: *mut u32 = unsafe {
            map_page(vmcb_phys_addr, Entry::FLAG_RW | Entry::FLAG_PWT).into()
        };
    }

    fn run_vessel(vmcb_addr: PhysAddr) {
        // just making sure the page is fine
        sanity_assert!(vmcb_addr.0 % BASIC_PAGE_SIZE == 0);

        unsafe {
            asm!(
                ""
            );
        };
        
    }
}

impl Svm {
    fn enable() {
        const SVME_BIT: u32 = 1 << 12;
    
        Self::check_support();
        Self::check_firmware_disabled();
    
        unsafe {
            let (mut low, high) = rdmsr(AmdMsr::Efer);
            low |= SVME_BIT;
            wrmsr(AmdMsr::Efer, low, high);
        }
    
        log_info!("Enabled SVM operation!");
    }
    /// Make sure SVM is supported on this CPU
    fn check_support() {
        const SVM_SUPPORT_ECX_BIT: u32 = 1 << 2;
    
        unsafe {
            assert!(
                __cpuid(0x8000_0001).ecx & SVM_SUPPORT_ECX_BIT != 0,
                "SVM isn't supported on this processor"
            );
        };
    }
    
    /// Perform a check to see if virtualization is disabled by the firmware.
    fn check_firmware_disabled() {
        // TODO: COrrect this shit
        // // TODO: Perform a check for TPM as well
        // const SVML_BIT: u32 = 1 << 2;
        //
        // unsafe {
        //     assert!(
        //         __cpuid(0x8000_000A).edx & SVML_BIT != 0,
        //         "SVM is disabled by firmware and thus cannot be enabled"
        //     );
        // };
    }
}

