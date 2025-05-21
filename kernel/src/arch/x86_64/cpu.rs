//! Assembly instruction wrappers & other low level CPU operations
//!
//! `NOTE:` `core::arch::x86_64` already implements `__cpuid`, `rdtsc` and many others, so use them when needed

use super::gdt::SegmentSelector;
use core::arch::asm;
use core::mem::transmute;

/// A shared trait for all MSRs
trait Msr {
    /// Get the address of that MSR
    fn address(self) -> u32;
}

// TODO: Make a shared MSRs enum

/// Intel CPUs specific MSRs
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntelMsr {
    /// Address of the `IA32_APIC_BASE` MSR
    Ia32ApicBase = 0x1B,
    /// Address of the `IA32_FEATURE_CONTROL` MSR
    Ia32FeatureControl = 0x3A,
    /// Address of the `IA32_VMX_BASIC` MSR
    Ia32VmxBasic = 0x480,
}

/// AMD CPUs specific MSRs
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AmdMsr {
    /// Extended feature enable
    Efer = 0xC000_0080,
    /// Control and status bits for SVM
    VmCr = 0xC001_0114,
    /// Physical address of the host state save area
    VmHsavePa = 0xC001_0117,
}

#[allow(unused)]
/// Wrapper for the 'outb' instruction, accessing a `u32` port
#[inline]
pub unsafe fn outb_32(port: u16, value: u32) {
    unsafe {
        asm! (
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack),
        );
    };
}

/// Wrapper for the 'out' instruction, accessing a `u8` port
#[inline]
pub unsafe fn outb_8(port: u16, value: u8) {
    unsafe {
        asm! (
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack),
        );
    };
}

#[allow(unused)]
/// Wrapper for the 'in' instruction, accessing a `u32` port
#[inline]
pub unsafe fn inb_32(port: u16) -> u32 {
    let res: u32;
    unsafe {
        asm! (
            "in eax, dx",
            out("eax") res,
            in("dx") port,
            options(nomem, nostack),
        );
    };

    res
}

/// Wrapper for the 'in' instruction, accessing a `u8` port
#[inline]
pub unsafe fn inb_8(port: u16) -> u8 {
    let res: u8;
    unsafe {
        asm! (
            "in al, dx",
            out("al") res,
            in("dx") port,
            options(nomem, nostack),
        );
    };

    res
}

/// Clear `RFLAGS` interrupt flag to mask all maskable external interrupts
#[inline]
pub fn cli() {
    unsafe {
        asm!("cli", options(nostack, nomem));
    };
}

/// Set `RFLAGS` interrupt flag to enable handling of external interrupts
#[inline]
pub fn sti() {
    unsafe {
        asm!("sti", options(nostack, nomem));
    };
}

/// Read the value of a model specific register (MSR)
#[inline]
pub unsafe fn rdmsr(msr: impl Msr) -> (u32, u32) {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            out("eax") low,
            out("edx") high,
            in("ecx") msr.address(),
            options(nostack, nomem),
        );
    };
    (low, high)
}

/// Write a value to a model specific register (MSR)
#[inline]
pub unsafe fn wrmsr(msr: impl Msr, low: u32, high: u32) {
    unsafe {
        asm!(
            "wrmsr",
            in("eax") low,
            in("edx") high,
            in("ecx") msr.address(),
            options(nostack, nomem),
        );
    };
}

#[inline]
pub fn get_rflags() -> u64 {
    let rflags: u64;
    unsafe {
        asm! (
            "pushfq",
            "pop {}",
            out(reg) rflags,
        );
    };

    rflags
}

impl Msr for IntelMsr {
    #[inline]
    fn address(self) -> u32 {
        self as u32
    }
}

impl Msr for AmdMsr {
    #[inline]
    fn address(self) -> u32 {
        self as u32
    }
}

macro_rules! define_register_reader {
    ($func_name:ident, $segment:ident, $raw:ty, $actual:ty) => {
        /// Read the value of the $segment register
        #[inline]
        pub fn $func_name() -> $actual {
            let segment: $raw;
            unsafe {
                asm! (
                    concat!("mov {}, ", stringify!($segment)),
                    out(reg) segment,
                    options(nostack),
                );

                transmute(segment)
            }
        }
    };
}

define_register_reader!(read_cs, cs, u16, SegmentSelector);
define_register_reader!(read_ds, ds, u16, SegmentSelector);
define_register_reader!(read_es, es, u16, SegmentSelector);
define_register_reader!(read_fs, fs, u16, SegmentSelector);
define_register_reader!(read_gs, gs, u16, SegmentSelector);
define_register_reader!(read_ss, ss, u16, SegmentSelector);
define_register_reader!(read_dr6, dr6, u32, u32);
define_register_reader!(read_dr7, dr7, u32, u32);

// TODO: Maybe implement these as functions with enums for the fields you can write to make it
// safer?

/// Macro to check whether a CR register number is valid during compile time
#[macro_export]
macro_rules! validate_cr {
    ($cr:literal) => {
        const _: () = assert!(
            matches!($cr, 0 | 2 | 3 | 4 | 8),
            "Invalid control register number. Must be 2, 3, 4, or 8.",
        );
    };
}

/// Wrapper to read the value of a control register
#[macro_export]
macro_rules! read_cr {
    ($cr:literal) => {{
        $crate::validate_cr!($cr);
        unsafe {
            let value: u64;
            core::arch::asm!(
                concat!("mov {}, cr", $cr),
                out(reg) value,
                options(nostack, nomem)
            );
            value
        }
    }};
}

/// Wrapper to write a value to a control register
#[macro_export]
macro_rules! write_cr {
    ($cr:literal, $val:expr) => {
        $crate::validate_cr!($cr);

        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            core::arch::asm!(
                concat!("mov cr", $cr, ", {}"),
                in(reg) $val,
                options(nostack, nomem)
            );
        }
    };
}
