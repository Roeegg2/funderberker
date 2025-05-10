//! HAV technology for Intel CPUs

use core::arch::x86_64::__cpuid;

use crate::{read_cr, write_cr};

/// Enables VMX on this processor
///
/// NOTE: In contrast to SVM, there is no way to make sure VMX isn't disabled in firmware before
/// executing VMRUN.
/// We can deduce that VMX isn't enabled if VMRUN triggers a PF
pub(super) fn enable() {
    const VMX_ENABLE_BIT: usize = 1 << 13;

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
