//! (Will be) safe, general arch abstractions so VMM doesn't need to deal with all the nitty gritty

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::init()
    };
}
