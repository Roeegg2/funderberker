//! This module contains implementations of drivers for various timers.

#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
pub mod hpet;
#[cfg(all(target_arch = "x86_64", feature = "pit"))]
pub mod pit;

// pub trait Timer: SpinLockDropable {
//     type TimerMode;
//
//     fn time_to_units(&self, time: Duration) -> u64;
//
//     fn alloc<'a>(time: Duration, mode: Self::TimerMode) -> Result<SpinLockGuard<'a, Self>, ()>;
//
//     unsafe fn init();
// }
