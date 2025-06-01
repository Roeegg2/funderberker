use core::arch::asm;

/// A shared trait for all MSRs
trait Msr {
    // TODO: Make this const when const trait fn
    /// Get the address of that MSR
    fn address(self) -> u32;
}

/// Intel CPUs specific MSRs
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum IntelMsr {
    /// Address of the `IA32_APIC_BASE` MSR
    Ia32ApicBase = 0x1B,
    /// Address of the `IA32_FEATURE_CONTROL` MSR
    Ia32FeatureControl = 0x3A,
    /// Address of the `IA32_VMX_BASIC` MSR
    Ia32VmxBasic = 0x480,
    /// Address of the `IA32_PAT` MSR
    Ia32Pat = 0x277,
}

/// AMD CPUs specific MSRs
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum AmdMsr {
    /// Extended feature enable
    Efer = 0xC000_0080,
    /// Control and status bits for SVM
    VmCr = 0xC001_0114,
    /// Physical address of the host state save area
    VmHsavePa = 0xC001_0117,
}

/// Data structure to hold the low and high parts of a model specific register (MSR)
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MsrData {
    /// Low 32 bits of the MSR
    pub low: u32,
    /// High 32 bits of the MSR
    pub high: u32,
}

pub struct Ia32ApicBase;
pub struct Ia32FeatureControl;
pub struct Ia32VmxBasic;

pub struct Efer;
pub struct VmCr;
pub struct VmHsavePa;

// TODO: Fix this 
impl Efer {
    /// System call extension enable
    pub const SCE: u64 = 1 << 0;
    /// Long mode enable
    pub const LME: u64 = 1 << 8;
    /// Long mode active
    pub const LMA: u64 = 1 << 10;
    /// No execute enable
    pub const NX: u64 = 1 << 11;
    /// SVM enable
    pub const SVM: u64 = 1 << 12;
    /// Long mode segment limit enable
    pub const LMSLE: u64 = 1 << 13;
    /// Fast FXSAVE/FXRSTOR enable
    pub const FFXSR: u64 = 1 << 14;
    /// Translation cache extension enable
    pub const TCE: u64 = 1 << 15;
    /// MCOMMIT instruction enable
    pub const MCOMMIT: u64 = 1 << 16;
    /// Interruptible WBINVD/WBINVDX enable
    pub const INTWB: u64 = 1 << 17;
    /// Upper address ignore enable
    pub const UAIE: u64 = 1 << 18;
    /// Automatic IBRS enable
    pub const AIBRSE: u64 = 1 << 19;

    /// All flags turned on
    pub const ALL: u64 = Self::SCE
        | Self::LME
        | Self::LMA
        | Self::NX
        | Self::SVM
        | Self::LMSLE
        | Self::FFXSR
        | Self::TCE
        | Self::MCOMMIT
        | Self::INTWB
        | Self::UAIE
        | Self::AIBRSE;
}

/// Read the value of a model specific register (MSR)
#[allow(private_bounds)]
#[inline]
pub unsafe fn rdmsr(msr: impl Msr) -> MsrData {
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

    MsrData {
        low,
        high,
    }
}

/// Write a value to a model specific register (MSR)
#[allow(private_bounds)]
#[inline]
pub unsafe fn wrmsr(msr: impl Msr, data: MsrData) {
    unsafe {
        asm!(
            "wrmsr",
            in("eax") data.low,
            in("edx") data.high,
            in("ecx") msr.address(),
            options(nostack, nomem),
        );
    };
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

impl Into<MsrData> for u64 {
    fn into(self) -> MsrData {
        MsrData {
            low: self as u32,
            high: (self >> 32) as u32,
        }
    }
}

impl Into<u64> for MsrData {
    fn into(self) -> u64 {
        ((self.high as u64) << 32) | (self.low as u64)
    }
}
