//! (Will be) safe, general arch abstractions so kernel doesn't need to deal with all the nitty gritty

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

/// The size of the smallest page that can be allocated
pub const BASIC_PAGE_SIZE: usize = 0x1000; // 4KB page size

// /// The size of the allocated stack for each core in `BASIC_PAGE_SIZE` pages
// pub const CORE_STACK_PAGE_COUNT: usize = 64; // 64KB stack for each core

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

    // /// Sets up a new stack for the current running core (BSP)
    // ///
    // /// NOTE: Make sure to mark this function as `#[inline(always)]` so we don't get any pops in
    // /// the code, which will try to access the old stack (which we aren't referencing anymore)
    // ///
    // /// SHOULD ONLY BE CALLED ONCE DURING BOOT!
    // unsafe fn migrate_to_new_stack();
}

/// Wrapper to call the arch specific `init` function
#[inline]
pub unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init();
    }
}

/// Wrapper to call the arch specific `init_cores` function
#[inline]
pub unsafe fn init_cores() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::X86_64::init_cores();
    }
}

// /// Wrapper to call the arch specific `migrate_to_new_stack` function
// #[inline(always)]
// pub unsafe fn migrate_to_new_stack() {
//     #[cfg(target_arch = "x86_64")]
//     unsafe {
//         x86_64::X86_64::migrate_to_new_stack();
//     }
// }
