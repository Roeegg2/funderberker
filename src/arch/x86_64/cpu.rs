//! Assembly instruction wrappers & other low level CPU operations
//! NOTE: 'core::arch::x86_64' already implements __cpuid, so use that when needed

use core::arch::asm;

#[allow(unused_imports)]
pub use core::arch::x86_64;

#[inline]
pub unsafe fn read_msr(msr: u32) -> (u32, u32) {
    let lo: u32;
    let hi: u32;
    asm! (
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
    );

    (lo, hi)
}

#[inline]
pub unsafe fn write_msr(msr: u32, hi: u32, lo: u32) {
    asm! (
        "wrmsr",
        in("ecx") msr,
        in("eax") lo,
        in("edx") hi,
        options(nomem, nostack),
    );
}

/// Wrapper for the 'outb' instruction
#[inline]
pub unsafe fn outb(port: u16, offset: u16, value: u8) {
    asm! (
        "out dx, al",
        in("dx") port + offset,
        in("al") value,
        options(nomem, nostack),
    );
}

/// Wrapper for the 'in' instruction
#[inline]
pub unsafe fn inb(port: u16, offset: u16) -> u8 {
    let res: u8;
    asm! (
        "in al, dx",
        out("al") res,
        in("dx") port + offset,
        options(nomem, nostack),
    );

    res
}

#[inline]
pub unsafe fn cli() {
    asm!("cli", options(nostack, nomem));
}

#[inline]
pub unsafe fn sti() {
    asm!("sti", options(nostack, nomem));
}
