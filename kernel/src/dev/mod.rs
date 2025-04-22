#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;
#[cfg(target_arch = "x86_64")]
pub mod time;
