//! HAV technology for Intel CPUs

use core::arch::x86_64::__cpuid;

use crate::{
    arch::x86_64::cpu::{Msr, rdmsr},
    read_cr, write_cr,
};

pub(super) fn start_operation() {

}

/// Enables VMX on this processor
pub(super) fn enable() {
    const VMX_ENABLE_BIT: usize = 1 << 13;

    check_firmware_disabled();
    check_support();

    // bit 4 of CR enabled VMX
    let mut cr4 = read_cr!(4);
    cr4 |= VMX_ENABLE_BIT;
    write_cr!(4, cr4);

    log_info!("Enabled VMX operation!");
}

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

/// Makes sure VMX isn't disabled in the firmware
fn check_firmware_disabled() {
    // TODO: Possibly check inside SMX as well. This is bit 1
    const OUTSIDE_SMX_DISABLE_BIT: u32 = 1 << 2;

    let (low, _) = unsafe { rdmsr(Msr::Ia32FeatureControl) };

    assert!(
        low & OUTSIDE_SMX_DISABLE_BIT != 0,
        "VMX outside SMX disabled by the firmware and thus cannot be enabled"
    );
}
