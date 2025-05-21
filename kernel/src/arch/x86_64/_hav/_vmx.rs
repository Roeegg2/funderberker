//! HAV technology for Intel CPUs

use core::{arch::x86_64::__cpuid, num::NonZero};
use core::arch::asm;
use utils::collections::fast_lazy_static::FastLazyStatic;
use crate::{
    arch::x86_64::{cpu::{rdmsr, IntelMsr}, paging::Entry}, mem::{pmm::{self, PmmAllocator}, vmm::{allocate_pages, map_page, translate}}, read_cr, write_cr
};
use super::Hav;

// TODO: Make this an actually invalid VMCS reviison
const INVALID_VMCS_REVISION: u32 = 0xffff_ffff;

static VMCS_REVISION: FastLazyStatic<u32> = FastLazyStatic::new(INVALID_VMCS_REVISION);

pub struct Vmx;

impl Hav for Vmx {
    fn start() {
        Self::enable();
        // XXX: Need to make sure this is UC, and watch out for different memory types other
        // revisions might have

        let phys_addr = pmm::get().allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap()).unwrap();

        let ptr: *mut u32 = unsafe {
            map_page(phys_addr, Entry::FLAG_RW).into()
        };
        println!("that's the ptr: {:?}", ptr);

        Self::find_revision_id();
        let revision = VMCS_REVISION.get();
        println!("thats the revision: {:?}", revision);
        unsafe {
            *ptr = revision;
        };

        println!("here!");
        unsafe {
            asm!(
                "vmxon [{}]",
                in(reg) &phys_addr.0
            );
        };
    }

    fn load_vessel() {
        
    }
}

impl Vmx {
    /// Make sure VMX is supported on this CPU
    fn check_support() {
        const VMX_SUPPORT_ECX_BIT: u32 = 1 << 5;
    
        unsafe {
            assert!(
                __cpuid(0x1).ecx & VMX_SUPPORT_ECX_BIT != 0,
                "VMX isn't supported on this processor"
            );
        };
    }
    
    /// Makes sure VMX isn't disabled by the firmware
    fn check_firmware_disabled() {
        // TODO: Possibly check inside SMX as well. This is bit 1
        const OUTSIDE_SMX_DISABLE_BIT: u32 = 1 << 2;
    
        let (low, _) = unsafe { rdmsr(IntelMsr::Ia32FeatureControl) };
    
        assert!(
            low & OUTSIDE_SMX_DISABLE_BIT != 0,
            "VMX outside SMX disabled by the firmware and thus cannot be enabled"
        );
    } 

    fn enable() {
        const VMX_ENABLE_BIT: usize = 1 << 13;

        // Make sure we indeed can enter VMX operation
        Self::check_support();
        Self::check_firmware_disabled();

        // bit 4 of CR enabled VMX
        let mut cr4 = read_cr!(4);
        cr4 |= VMX_ENABLE_BIT;
        write_cr!(4, cr4);

        log_info!("Enabled VMX operation!");
    }

    fn find_revision_id() {
        // The revision ID is 30 bits
        const MASK: u32 = 0xffff_ffff >> 1;

        unsafe {
            let revision = rdmsr(IntelMsr::Ia32VmxBasic).0 & MASK;

            VMCS_REVISION.set(revision);
        };
    }
}

// pub(super) fn start_operation() {
//     // let phys_addr = pmm::get().allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap()).unwrap();
//     // let ptr = unsafe {map_page(phys_addr, Entry::FLAGS_NONE)};
//
//     // TODO: Make this writeback cacheable
//     let virt_addr = allocate_pages(1, Entry::FLAGS_NONE);
//     let phys_addr = translate(virt_addr).unwrap();
//
//     let revision = unsafe {rdmsr(IntelMsr::Ia32VmxBasic).0} & 0x7FFFFFFF;
//     unsafe {
//         let ptr: *mut u32 = virt_addr.into();
//         *ptr = revision
//     };
//
//     unsafe {
//         core::arch::asm!("vmxon {}", in(reg) phys_addr.0);
//     }
//
//     // map the page
//     // fill in the revision Id
//     // execute vmxon
//
// }
//
//
//
// fn set_vmcs_revision
