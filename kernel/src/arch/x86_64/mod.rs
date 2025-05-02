//! Everything specific to x86_64 arch

use super::{Architecture, CORE_STACK_PAGE_COUNT};
use crate::mem::vmm::allocate_pages;
use interrupts::Idt;
use paging::Entry;

use core::arch::asm;

#[macro_use]
pub mod cpu;
pub mod apic;
pub mod interrupts;
mod isrs;
// #[cfg(feature = "mp")]
// mod mp;
pub mod paging;

/// a ZST to implement the Arch trait on
pub(super) struct X86_64;

/// Pointer to some descriptor table (IDTR, GDTR, etc)
#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct DescriptorTablePtr {
    limit: u16,
    base: u64,
}

impl Architecture for X86_64 {
    unsafe fn init() {
        unsafe {
            // Make sure no pesky interrupt interrupt us
            cpu::cli();
            Idt::init();
            cpu::sti();
        };
    }

    #[cfg(feature = "mp")]
    #[inline]
    unsafe fn init_cores() {
        // mp::init_cores();
        // make sure we are on an MP system, otherwise return
        //
    }

    #[inline(always)]
    unsafe fn migrate_to_new_stack() {
        let new_stack: *const () = allocate_pages(CORE_STACK_PAGE_COUNT, Entry::FLAG_RW).into();
        unsafe {
            asm!(
                "mov rsp, {0}",
                in(reg) new_stack.addr(),
                options(nostack)
            );
        }
        // allocate pages of `STACK_SIZE` bytes
        // reload RSP with the new stack
    }
}
