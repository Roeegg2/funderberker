#![cfg_attr(not(test), no_std)]
#![feature(let_chains)]

pub mod collections;
//#[macro_use]
pub mod mem;

#[macro_export]
macro_rules! const_max {
    ($a:expr, $b:expr) => {
        if $a > $b { $a } else { $b }
    };
}

// TODO: Rework this
#[macro_export]
macro_rules! sum_fields {
    ($struct:ident { $($field:ident),* }) => {
        impl $struct {
            pub fn sum_fields(&self) -> usize {
                0 $(+ self.$field as usize)*
            }
        }
    };
}
