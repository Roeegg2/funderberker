#![no_std]
#![feature(sync_unsafe_cell)]
// TODO: Remove this once the modular_bitfield errors are taken care of
#![allow(dead_code)]
// TODO: Remove this once you fix the `as` conversion warnings
#![allow(clippy::cast_possible_truncation)]

pub mod arch;
pub mod mem;
