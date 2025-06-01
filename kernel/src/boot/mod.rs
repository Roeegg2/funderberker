//! Boot specific stuff for various boot methods

// TODO: Some boot sanity checks to make sure basic features that are expected are available on
// this CPU.

#[cfg(feature = "limine")]
pub mod limine;
