use alloc::boxed::Box;
use utils::collections::id::Id;

// TODO: make sure only one scheduler type is enabled

#[cfg(feature = "scheduler_constant")]
pub mod constant;

pub trait Schedulable {
    fn id(&self) -> Id;

    // TODO: Add `Context` struct to store info and shit
}

pub trait Scheduler<T>
where
    T: Schedulable,
{
    type ParametersForNew;

    fn new(params: Self::ParametersForNew) -> Self;

    fn add(&mut self, vessel: Box<T>);

    fn remove(&mut self) -> Box<T>;

    fn operation_loop(&mut self) -> !;
}
