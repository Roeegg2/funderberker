//! Hardware Assisted Virtualization support for x86_64

use super::{CpuVendor, CPU_VENDOR};

pub mod svm;
// pub mod vmx;

pub trait Hav {
    fn start();

    fn load_vessel();
}

// // Enable HAV
// pub fn enable() {
//     match CPU_VENDOR.get() {
//         CpuVendor::Intel => {
// vmx::enable();
// vmx::start_operation();
//         },
//         CpuVendor::Amd => svm::enable(),
//         _ => unreachable!(),
//     }
// }
