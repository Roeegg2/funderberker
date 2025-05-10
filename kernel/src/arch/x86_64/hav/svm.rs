//! HAV technology for AMD CPUs

use core::arch::x86_64::__cpuid;

use crate::arch::x86_64::cpu::{Msr, rdmsr, wrmsr};

/// Enables SVM on this processor
pub(super) fn enable() {
    const ENABLE_BIT: u32 = 1 << 12;

    check_support();
    check_bios_disabled();

    unsafe {
        let (mut low, high) = rdmsr(Msr::Efer);
        low |= ENABLE_BIT;
        wrmsr(Msr::Efer, low, high);
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

/// AMD CPUs can perform a check to see if virtualization is disabled by the firmware.
fn check_bios_disabled() {
    const BIOS_DISABLED_BIT: u32 = 4;

    let (low, _) = unsafe { rdmsr(Msr::VmCr) };

    assert!(
        low & BIOS_DISABLED_BIT == 0,
        "SVM/VMX is disabled in BIOS and thus cannot be enabled"
    );
}
