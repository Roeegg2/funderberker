//! Wrapper for handling the TSC

// TODO: Make a standardized timer interface and implement it for TSC too

use core::{arch::x86_64::__cpuid, hint};

use super::cpu::rdtsc;

// TODO: Use a set once or something like that for this
static mut TSC_FREQ: u64 = 0;

// TODO: Should maybe return an error instead of just asserting?
/// Initialize the TSC for use.
///
/// SHOULD ONLY BE CALLED ONCE DURING BOOT!
pub(super) unsafe fn init() {
    // Just make sure the TSC is not disabled in CR4
    // write_cr!(cr4, read_cr!(cr4) & !0b100);

    // Calculate the frequency of the TSC
    unsafe {TSC_FREQ = calculate_frequency() };
} 

/// Calculate the frequency of the TSC
fn calculate_frequency() -> u64 {
    // TODO: Should probably check to make sure that these CPUID leaves are supported
    
    const CORE_CRYSTAL_FREQ_INFO: u32 = 0x15;
    const PROCESSOR_AND_BUS_FREQ_INFO: u32 = 0x16;

    // First of all, try getting it using the designated CPUID leaf
    let (denominator, numerator, mut mult) = {
        let ret = unsafe { __cpuid(CORE_CRYSTAL_FREQ_INFO) };
        (ret.eax, ret.ebx, ret.ecx as u64)
    };

    // Just sanity checking
    utils::sanity_assert!(denominator != 0);
    utils::sanity_assert!(numerator != 0);

    // If that didn't work, it should be calculated using the processor and bus frequency info
    if mult == 0 {
        let processor_base_freq = unsafe { __cpuid(PROCESSOR_AND_BUS_FREQ_INFO).eax };

        mult = (processor_base_freq as u64 * 10_000_000) * ((denominator / numerator) as u64);

        utils::sanity_assert!(mult != 0);
    }

    (denominator as u64 / numerator as u64) * mult as u64
}

/// Spin for the given amount of time in microseconds
#[inline(always)]
pub(super) fn us_spin(time: u64) {
    let cycles = time * unsafe { TSC_FREQ };

    let start = unsafe { rdtsc() };
    loop {
        hint::spin_loop();
        if unsafe { rdtsc() } - start >= cycles {
            break;
        }
    }
}

/// Spin for the given amount of time in milliseconds 
#[inline(always)]
pub(super) fn ms_spin(time: u64) {
    us_spin(time * 1000);
}
