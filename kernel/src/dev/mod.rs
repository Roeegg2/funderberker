//! Various drivers and driver interfaces

pub mod clock;
#[cfg(feature = "legacy")]
pub mod cmos;
#[cfg(feature = "framebuffer")]
pub mod framebuffer;
#[cfg(feature = "serial")]
pub mod serial;
pub mod timer;
