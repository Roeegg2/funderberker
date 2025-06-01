//! Assembly instruction wrappers & other low level CPU operations
//!
//! `NOTE:` `core::arch::x86_64` already implements `__cpuid`, `rdtsc` and many others, so use them when needed

use modular_bitfield::prelude::*;

use crate::mem::VirtAddr;

use core::{arch::asm, mem::transmute};

pub mod msr;

pub trait Register {
    /// Read the value of the control register
    unsafe fn read() -> Self;

    /// Write a value to the control register
    unsafe fn write(self);
}

#[derive(Clone, Copy)]
#[bitfield]
#[repr(u64)]
pub struct Rflags {
    /// Carry flag
    pub cf: B1,
    /// Reserved (must be 1)
    pub reserved_mbo: B1,
    /// Parity flag
    pub pf: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_0: B1,
    /// Auxiliary carry flag
    pub af: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_1: B1,
    /// Zero flag
    pub zf: B1,
    /// Sign flag
    pub sf: B1,
    /// Trap flag
    pub tf: B1,
    /// Interrupt enable flag
    pub if_enable: B1,
    /// Direction flag
    pub df: B1,
    /// Overflow flag
    pub of: B1,
    /// I/O privilege level
    pub iopl: B2,
    /// Nested task flag
    pub nt: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_2: B1,
    /// Resume flag
    pub rf: B1,
    /// Virtual 8086 mode flag
    pub vm: B1,
    /// Alignment check flag
    pub ac: B1,
    /// Virtual interrupt flag
    pub vif: B1,
    /// Virtual interrupt pending flag
    pub vip: B1,
    /// ID flag
    pub id: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_3: B42,
}

/// The CR0 register
#[derive(Clone, Copy)]
#[bitfield]
#[repr(u64)]
pub struct Cr0 {
    /// Protection enable
    pub pe: B1,
    /// Monitor coprocessor
    pub mp: B1,
    /// Emulation
    pub em: B1,
    /// Task switched
    pub ts: B1,
    /// Extension type
    pub et: B1,
    /// Numeric error
    pub ne: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_0: B10,
    /// Write protect
    pub wp: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_1: B1,
    /// Alignment mask
    pub am: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_2: B10,
    /// Not write through
    pub nw: B1,
    /// Cache disable
    pub cd: B1,
    /// Paging enable
    pub pg: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_3: B32,
}

/// The CR2 register
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct Cr2(pub u64);

/// The CR3 register
#[bitfield]
#[repr(u64)]
pub struct Cr3 {
    /// Reserved (must be 0)
    pub reserved_mbz_0: B3,
    /// Page-level write-through
    pub pwt: B1,
    /// Page-level cache disable
    pub pcd: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_1: B7,
    /// PML4/PML5 base physical address
    pub top_pml: B52,
}

/// The CR4 register
#[bitfield]
#[repr(u64)]
pub struct Cr4 {
    /// Virtual 8086 mode extensions
    pub vme: B1,
    /// Protected-mode virtual interrupts
    pub pvi: B1,
    /// Time stamp disable
    pub tsd: B1,
    /// Debugging extensions
    pub de: B1,
    /// Page size extension
    pub pse: B1,
    /// Physical address extension
    pub pae: B1,
    /// Machine check enable
    pub mce: B1,
    /// Page global enable
    pub pge: B1,
    /// Performance-monitoring counter enable
    pub pce: B1,
    /// Operating system support for FXSAVE and FXRSTOR
    pub osfxsr: B1,
    /// Operating system support for unmasked SIMD floating-point exceptions
    pub osxmmexcpt: B1,
    /// User-mode instruction prevention
    pub umip: B1,
    /// 57-bit linear addresses
    pub la57: B1,
    /// VMX enable
    pub vmxe: B1,
    /// SMX enable
    pub smxe: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_0: B1,
    /// FSGSBASE enable
    pub fsgsbase: B1,
    /// PCID enable
    pub pcide: B1,
    /// XSAVE and processor extended states enable
    pub osxsave: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_1: B1,
    /// SMEP enable
    pub smep: B1,
    /// SMAP enable
    pub smap: B1,
    /// Protection key enable
    pub pke: B1,
    /// Control-flow enforcement technology
    pub cet: B1,
    /// Protection key for supervisor-mode pages
    pub pks: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_2: B39,
}

// TODO: Might be slight differences between AMD and Intel

/// Debug register 0
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct Dr0(VirtAddr);

/// Debug register 1
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct Dr1(VirtAddr);

/// Debug register 2
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct Dr2(VirtAddr);

/// Debug register 3
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct Dr3(VirtAddr);

/// Debug register 6 on AMD CPUs
#[derive(Clone, Copy)]
#[bitfield]
#[repr(u64)]
pub struct AmdDr6 {
    /// Debug register 0 condition
    pub bd0: B1,
    /// Debug register 1 condition
    pub bd1: B1,
    /// Debug register 2 condition
    pub bd2: B1,
    /// Debug register 3 condition
    pub bd3: B1,
    /// Reserved bits (must be 1)
    pub reserved_mbo_0: B7,
    /// Bus lock detected
    pub bld: B1,
    /// Reserved bits (must be 0)
    pub reserved_mbz_0: B1,
    /// Breakpoint debug access detected
    pub bd: B1,
    /// Breakpoint single step
    pub bs: B1,
    /// Breakpoint task switch
    pub bt: B1,
    /// Reserved bits (must be 1)
    pub reserved_mbo_1: B16,
    /// Reserved bits (must be 0)
    pub reserved_mbz_1: B32,
}

/// Debug register 7 on AMD CPUs, 64 bit
#[derive(Clone, Copy)]
#[bitfield]
#[repr(u64)]
pub struct AmdDr7 {
    /// Local breakpoint 0 enable
    pub l0: B1,
    /// Global breakpoint 0 enable
    pub g0: B1,
    /// Local breakpoint 0 enable
    pub l1: B1,
    /// Global breakpoint 0 enable
    pub g1: B1,
    /// Local breakpoint 0 enable
    pub l2: B1,
    /// Global breakpoint 0 enable
    pub g2: B1,
    /// Local breakpoint 0 enable
    pub l3: B1,
    /// Global breakpoint 0 enable
    pub g3: B1,
    /// Local breakpoint enable
    pub le: B1,
    /// Global breakpoint enable
    pub ge: B1,
    /// Reserved (must be 1)
    pub reserved_mbo: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_0: B2,
    /// General detect enabled
    pub gd: B1,
    /// Reserved (must be 0)
    pub reserved_mbz_1: B2,
    /// Type of Transactions to Trap 0
    pub ttt_0: B2,
    /// Length of breakpoint 0
    pub lb_0: B2,
    /// Type of Transactions to Trap 0
    pub ttt_1: B2,
    /// Length of breakpoint 0
    pub lb_1: B2,
    /// Type of Transactions to Trap 0
    pub ttt_2: B2,
    /// Length of breakpoint 0
    pub lb_2: B2,
    /// Type of Transactions to Trap 0
    pub ttt_3: B2,
    /// Length of breakpoint 0
    pub lb_3: B2,
    /// Reserved (must be 0)
    pub reserved_mbz_2: B32,
}


/// Wrapper for the 'outb' instruction, accessing a `u32` port
#[allow(unused)]
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

/// Read the current stack pointer (RSP) register
pub fn read_rsp() -> usize {
    let rsp: u64;
    unsafe {
        asm!("mov {:r}, rsp", out(reg) rsp);
    }
    
    rsp as usize
}

/// Write a new value to the stack pointer (RSP) register
pub unsafe fn write_rsp(addr: VirtAddr) {
    unsafe {
        asm!("mov rsp, {:r}", in(reg) addr.0);
    }
}

impl Register for Dr0 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr0", out(reg) value);
        }
        
        Dr0(VirtAddr(value as usize))
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov dr0, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Dr1 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr1", out(reg) value);
        }
        
        Dr1(VirtAddr(value as usize))
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov dr1, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Dr2 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr2", out(reg) value);
        }
        
        Dr2(VirtAddr(value as usize))
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov dr2, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Dr3 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr3", out(reg) value);
        }
        
        Dr3(VirtAddr(value as usize))
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov dr3, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for AmdDr6 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr6", out(reg) value);
        }
        
        AmdDr6::from_bytes(value.to_le_bytes())
    }

    unsafe fn write(self) {
        // AMD DR6 is read-only, writing is not allowed
        panic!("AMD DR6 is read-only");
    }
}

impl Register for AmdDr7 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, dr7", out(reg) value);
        }
        
        AmdDr7::from_bytes(value.to_le_bytes())
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov dr7, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Rflags {
    unsafe fn read() -> Self {
        let rflags: u64;
        unsafe {
            asm!("pushfq", "pop {:r}", out(reg) rflags);
        }
        
        rflags.into()
    }

    unsafe fn write(self) {
        unsafe {
            asm!("push {:r}", "popfq", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Cr0 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, cr0", out(reg) value);
        }
        
        value.into()
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov cr0, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Cr2 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, cr2", out(reg) value);
        }
        
        Cr2(value)
    }

    unsafe fn write(self) {
        // CR2 is read-only, writing is not allowed
        panic!("CR2 is read-only");
    }
}

impl Register for Cr3 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, cr3", out(reg) value);
        }
        
        value.into()
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov cr3, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

impl Register for Cr4 {
    unsafe fn read() -> Self {
        let value: u64;
        unsafe {
            asm!("mov {:r}, cr4", out(reg) value);
        }
        
        value.into()
    }

    unsafe fn write(self) {
        unsafe {
            asm!("mov cr4, {:r}", in(reg) transmute::<Self, u64>(self));
        }
    }
}

// impl Into<u64> for MsrData {
//     fn into(self) -> u64 {
//         ((self.high as u64) << 32) | (self.low as u64)
//     }
// }
