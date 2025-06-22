#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use utils::collections::id::Id;

// TODO: make sure only one scheduler type is enabled

#[cfg(feature = "constant")]
pub mod constant;

/// A trait for types that can be scheduled by one of the available schedulers.
pub trait Schedulable {
    /// Get the ID of the schedulable.
    fn id(&self) -> Id;

    /// Run the schedulable
    fn run(&mut self);
    // TODO: Add `Context` struct to store info and shit
}

pub trait Scheduler<T>
where
    T: Schedulable,
{
    /// The parameters required to create a new scheduler.
    type ParametersForNew;

    /// Create a new scheduler with the given parameters.
    fn new(params: Self::ParametersForNew) -> Self;

    /// Add a new vessel to the scheduling queue.
    fn add(&mut self, vessel: Box<T>);

    /// Remove a vessel from the scheduling queue.
    fn remove(&mut self) -> Box<T>;

    /// Enter the operation loop of the scheduler.
    fn operation_loop(&mut self) -> !;
}
