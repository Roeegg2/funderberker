//! Simple scheduler which runs a single constant vessel

use alloc::boxed::Box;
use utils::sanity_assert;

use crate::sync::spinlock::SpinLockDropable;

use super::{Schedulable, Scheduler};

pub type ParametersForNew<T> = Option<Box<T>>;

pub struct Constant<T>
where
    T: Schedulable,
{
    vessel: Option<Box<T>>,
}

impl<T> Constant<T>
where
    T: Schedulable,
{
    // TODO: Remove this `new_const` when we get const fn in trait support, and use `new` instead
    pub const fn new_const() -> Self {
        Self { vessel: None }
    }
}

impl<T> Scheduler<T> for Constant<T>
where
    T: Schedulable,
{
    type ParametersForNew = ParametersForNew<T>;

    fn new(params: Self::ParametersForNew) -> Self {
        Self { vessel: params }
    }

    fn add(&mut self, vessel: Box<T>) {
        // sanity_assert!("Tried to add an additional schedulable but this is the 'const' scheduler");
        sanity_assert!(self.vessel.is_none());
        self.vessel = Some(vessel);
    }

    fn remove(&mut self) -> Box<T> {
        self.vessel
            .take()
            .expect("Tried to expel an additional schedulable but this is the 'const' scheduler")
    }

    fn operation_loop(&mut self) -> ! {
        log_info!("Entered scheduler loop");
        loop {
            // jump to the running point stored in the Context
        }
    }
}

unsafe impl<T> Sync for Constant<T> where T: Schedulable {}
unsafe impl<T> Send for Constant<T> where T: Schedulable {}

impl<T> SpinLockDropable for Constant<T> where T: Schedulable {}
