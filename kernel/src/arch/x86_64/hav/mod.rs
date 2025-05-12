//! Hardware Assisted Virtualization support for x86_64

use super::{CpuVendor, get_cpu_vendor};

mod svm;
mod vmx;

// Enable HAV
pub fn enable() {
    match get_cpu_vendor() {
        CpuVendor::Intel => {
vmx::enable();
vmx::start_operation();
        },
        CpuVendor::Amd => svm::enable(),
        _ => unreachable!(),
    }
}
