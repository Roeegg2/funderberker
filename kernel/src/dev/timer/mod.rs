//! This module contains implementations of drivers for various timers.

use core::time::Duration;

use crate::arch::x86_64::{apic::ioapic, interrupts};

#[cfg(all(target_arch = "x86_64"))]
pub mod apic;
#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
pub mod hpet;
#[cfg(all(target_arch = "x86_64", feature = "pit"))]
pub mod pit;

/// Possible errors that a timer might encounter
#[derive(Debug, Clone, Copy)]
pub enum TimerError {
    /// The time period is invalid
    InvalidTimePeriod,
    /// No timer is currently available for allocation
    NoTimerAvailable,
    /// Timer mode isn't supported by the hardaware
    UnsupportedTimerMode,
}

/// A trait that represents a timer. This trait is implemented by all timers in the system.
///
/// **VERY IMPORTATNT NOTE:**
/// The timers are getting disabled automatically on `drop()`, so you need to absolutely make sure that the
/// timer instance is alive for the entire time you need it.
///
/// I know this is annoying behaviour, but this is neccesary to make sure that there is no "timer
/// leaking", otherwise the timer couldn't be used in the future.
pub trait Timer: Sized {
    type TimerMode: Copy + Clone;
    /// Setup and start the timer
    fn start(&mut self, time: Duration, timer_mode: Self::TimerMode) -> Result<(), TimerError>;

    /// Disable/enable the timer
    fn set_disabled(&mut self, disable: bool);
}

/// Initializes either HPET or the PIT
pub fn init_secondary_timer() {
    unsafe {
        ioapic::set_disabled(interrupts::PIT_IRQ, false).expect("Failed to set PIT IRQ disabled");
    }
}
