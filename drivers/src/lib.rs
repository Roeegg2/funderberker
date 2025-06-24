//! Various drivers and driver interfaces
#![no_std]
// TODO: Remove this once the modular_bitfield errors are taken care of
#![allow(dead_code)]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

extern crate alloc;

pub mod bus;
pub mod clock;
pub mod storage;
pub mod timer;
