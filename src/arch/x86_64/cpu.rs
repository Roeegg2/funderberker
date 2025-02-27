//! Assembly instruction wrappers & other low level CPU operations

use core::arch::asm;

// NOTE: 'core::arch::x86_64' already implements __cpuid, so use that when needed
#[allow(unused_imports)]
pub use core::arch::x86_64;

/// Wrapper for the 'outb' instruction
#[inline]
pub unsafe fn outb(port: u16, offset: u16, value: u8) {
    unsafe {
        asm! (
            "out dx, al",
            in("dx") port + offset,
            in("al") value,
            options(nomem, nostack),
        )
    };
}

/// Wrapper for the 'in' instruction
#[inline]
pub unsafe fn inb(port: u16, offset: u16) -> u8 {
    let res: u8;
    unsafe {
        asm! (
            "in al, dx",
            out("al") res,
            in("dx") port + offset,
            options(nomem, nostack),
        )
    };

    res
}

/// Clear `RFLAGS` interrupt flag to mask all maskable external interrupts
#[inline]
pub unsafe fn cli() {
    unsafe { asm!("cli", options(nostack, nomem)) };
}

/// Set `RFLAGS` interrupt flag to enable handling of external interrupts
#[inline]
pub unsafe fn sti() {
    unsafe { asm!("sti", options(nostack, nomem)) };
}

#[macro_export]
macro_rules! read_cr {
    (cr0) => {{
        let value: usize;
        unsafe {
        core::arch::asm!(
            "mov {}, cr0",
            out(reg) value,
            options(nostack, nomem)
        );
        };
        value
    }};
    (cr2) => {{
        let value: usize;
        unsafe {
        core::arch::asm!(
            "mov {}, cr2",
            out(reg) value,
            options(nostack, nomem)
        );
        };
        value
    }};
    (cr3) => {{
        let value: usize;
        unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) value,
            options(nostack, nomem)
        );
        };
        value
    }};
    (cr4) => {{
        let value: usize;
        unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) value,
            options(nostack, nomem)
        );
        };
        value
    }};
    ($cr:expr) => {
        compile_error!("Only cr0, cr2, cr3, and cr4 are supported.");
    };
}

#[macro_export]
macro_rules! write_cr {
    (cr0, $val:expr) => {{
        unsafe {
        core::arch::asm!(
            "mov cr0, {}",
            in(reg) $val,
            options(nostack, nomem)
        );
        };
    }};
    (cr2, $val:expr) => {{
        unsafe {
        core::arch::asm!(
            "mov cr2, {}",
            in(reg) $val,
            options(nostack, nomem)
        );
        };
    }};
    (cr3, $val:expr) => {{
        unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) $val,
            options(nostack, nomem)
        );
        };
    }};
    (cr4, $val:expr) => {{
        unsafe {
        core::arch::asm!(
            "mov cr4, {}",
            in(reg) $val,
            options(nostack, nomem)
        );
        };
    }};
    () => {
        compile_error!("Only cr0, cr2, cr3, and cr4 are supported.");
    };
}
