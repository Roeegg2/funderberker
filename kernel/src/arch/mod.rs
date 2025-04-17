//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub use x86_64::paging::BASIC_PAGE_SIZE;

/// A trait that every arch should implement
trait Architecture {
    /// Initilize everything arch related
    unsafe fn init();

    /// Read the global timer
    fn cycles_since_boot() -> u64;

    /// Initialize the other cores on the system
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    #[cfg(feature = "mp")]
    unsafe fn init_cores();
}

#[inline]
pub unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init();
    }
}

#[inline(always)]
pub fn cycles_since_boot() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return x86_64::X86_64::cycles_since_boot();
}

#[inline]
pub unsafe fn init_cores() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init_cores();
    }
}

// TODO: Move this
pub fn ms_wait(milliseconds: usize) {
    core::arch::x86_64::_rd
}
