#![cfg_attr(not(test), no_std)]
#![feature(let_chains)]
#![feature(box_vec_non_null)]

pub mod collections;
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

#[macro_export]
macro_rules! ptr_add_layout {
    ($ptr:expr, $i:expr, $layout:expr, $type:ty) => {
        //let _: usize = $i;
        //let _: Layout = $layout;
        $ptr.cast::<u8>().add($i * $layout.size()).cast::<$type>()
    };
}
