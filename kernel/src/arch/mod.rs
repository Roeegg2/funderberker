//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub use x86_64::paging::BASIC_PAGE_SIZE;

/// A trait that every arch should implement
trait Architecture {
    /// Initilize everything arch related
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    unsafe fn init();

    /// Initialize the other cores on the system
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    #[cfg(feature = "mp")]
    unsafe fn init_cores();
}

/// Wrapper to call the arch specific init function
#[inline]
pub unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init();
    }
}

/// Wrapper to call the arch specific init_cores function
#[inline]
pub unsafe fn init_cores() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init_cores();
    }
}
