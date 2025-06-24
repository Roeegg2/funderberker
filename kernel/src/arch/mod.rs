//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

use crate::mem::paging::PagingManager;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub const BASIC_PAGE_SIZE: usize = x86_64::X86_64::BASIC_PAGE_SIZE.size();

/// A trait that every arch should implement
// TODO: Make this internal
pub trait Arch: PagingManager + Sized {
    /// Initilize everything arch related
    ///
    /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    unsafe fn early_boot_init();
}
