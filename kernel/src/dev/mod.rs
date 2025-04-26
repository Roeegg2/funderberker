pub mod clock;
pub mod cmos;
#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;
pub mod timer;
