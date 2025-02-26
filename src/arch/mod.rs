#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub(super) unsafe fn init() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        x86_64::init()
    };
}
