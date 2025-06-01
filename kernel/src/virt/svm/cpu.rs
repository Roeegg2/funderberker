use utils::sanity_assert;

use crate::{arch::BASIC_PAGE_SIZE, mem::PhysAddr};

use core::arch::asm;

#[inline]
pub(super) unsafe fn vmrun(vmcb: PhysAddr) {
    sanity_assert!(vmcb.0 % BASIC_PAGE_SIZE == 0);

    unsafe {
        asm!(
            "vmrun",
            in("rax") vmcb.0,
        );
    };
}
