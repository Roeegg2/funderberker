//! Assembly instruction wrappers & other low level CPU operations

use core::arch::asm;

// NOTE: 'core::arch::x86_64' already implements __cpuid, so use that when needed
#[allow(unused_imports)]
pub use core::arch::x86_64;

/// Wrapper for the 'outb' instruction
#[cfg(feature = "serial")]
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
#[cfg(feature = "serial")]
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
    ($cr:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let value: usize;
            core::arch::asm!(
                concat!("mov {}, ", stringify!($cr)),
                out(reg) value,
                options(nostack, nomem)
            );
            value
        }
    }};
}

#[macro_export]
macro_rules! write_cr {
    ($cr:ident, $val:expr) => {{
        #[allow(unused_unsafe)]
        unsafe {
            core::arch::asm!(
                concat!("mov ", stringify!($cr), ", {}"),
                in(reg) $val,
                options(nostack, nomem)
            );
        }
    }};
}
