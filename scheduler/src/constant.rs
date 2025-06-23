//! Simple scheduler which runs a single constant vessel

use super::{Schedulable, Scheduler};
use alloc::boxed::Box;
use logger::*;
use utils::{sanity_assert, sync::spinlock::SpinLockable};

/// The parameters required to create a new `Constant` scheduler.
pub type ParametersForNew<T> = Option<Box<T>>;

/// A simple scheduler that runs a single constant vessel.
///
/// Used for testing purposes
pub struct Constant<T>
where
    T: Schedulable,
{
    scheduable: Option<Box<T>>,
}

impl<T> Constant<T>
where
    T: Schedulable,
{
    // TODO: Remove this `new_const` when we get const fn in trait support, and use `new` instead
    pub const fn new_const() -> Self {
        Self { scheduable: None }
    }
}

impl<T> Scheduler<T> for Constant<T>
where
    T: Schedulable,
{
    type ParametersForNew = ParametersForNew<T>;

    fn new(params: Self::ParametersForNew) -> Self {
        Self { scheduable: params }
    }

    fn add(&mut self, vessel: Box<T>) {
        // sanity_assert!("Tried to add an additional schedulable but this is the 'const' scheduler");
        sanity_assert!(self.scheduable.is_none());
        self.scheduable = Some(vessel);
    }

    fn remove(&mut self) -> Box<T> {
        self.scheduable
            .take()
            .expect("Tried to expel an additional schedulable but this is the 'const' scheduler")
    }

    fn operation_loop(&mut self) -> ! {
        log_info!("Entered scheduler loop");

        if let Some(ref mut vessel) = self.scheduable {
            loop {
                vessel.run();
            }
        } else {
            panic!("No schedulable found in the constant scheduler");
        }
    }
}

unsafe impl<T> Sync for Constant<T> where T: Schedulable {}
unsafe impl<T> Send for Constant<T> where T: Schedulable {}

impl<T> SpinLockable for Constant<T> where T: Schedulable {}
