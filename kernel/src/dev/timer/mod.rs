//! This module contains implementations of drivers for various timers.

use core::time::Duration;

#[cfg(target_arch = "x86_64")]
pub mod apic;
#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
pub mod hpet;
#[cfg(all(target_arch = "x86_64", feature = "legacy"))]
pub mod pit;

const PIT_IRQ: u8 = 0;
const RTC_IRQ: u8 = 8;

/// Possible errors that a timer might encounter
#[derive(Debug, Clone, Copy)]
pub enum TimerError {
    /// The time period is invalid
    InvalidTimePeriod,
    /// No timer is currently available for allocation
    NoTimerAvailable,
    /// Timer mode isn't supported by the hardaware
    UnsupportedTimerMode,
    /// Invalid timer flags passed
    InvalidTimerFlags,
    /// An error encountered while working with the IDT
    IdtError,
    /// IRQ mapping error (eg not IRQ lines available, IRQ line already taken)
    IrqError,
}

/// A trait that represents a timer. This trait is implemented by all timers in the system.
///
/// *VERY IMPORTATNT NOTE:*
/// The timers are getting disabled automatically on `drop()`, so you need to absolutely make sure that the
/// timer instance is alive for the entire time you need it.
///
/// I know this is annoying behaviour, but this is neccesary to make sure that there is no "timer
/// leaking", otherwise the timer couldn't be used in the future.
pub trait Timer: Sized {
    /// The possible modes the timer supports (ie. `OneShot`, `Periodic`, etc)
    type TimerMode: Copy + Clone;
    /// Additional custom flags and possible configuration options
    type AdditionalConfig;

    /// Configure and setup the timer, and return the amount of clock ticks that the timer will
    /// tick for
    fn configure(
        &mut self,
        time: Duration,
        timer_mode: Self::TimerMode,
        additional_config: Self::AdditionalConfig,
    ) -> Result<u64, TimerError>;

    /// Disable/enable the timer
    fn set_disabled(&mut self, disable: bool);
}

// /// Initializes either HPET or the PIT
// pub fn enable_secondary_timer() {
//     unsafe {
//         ioapic::set_disabled(interrupts::PIT_IRQ, false).expect("Failed to set PIT IRQ disabled");
//     }
// }
