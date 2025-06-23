//! Pat support for `x86_64` paging

use core::arch::x86_64::__cpuid;

use crate::arch::x86_64::cpu::msr::{IntelMsr, rdmsr, wrmsr};

use super::{Entry, PageSize};

/// The amount of bits between each PAT entry in the `IA32_PAT` MSR. This is the amount of bits we
/// need to shift to access each PAT entry.
const SHIFTING_SIZE: u8 = 8;

const DEFAULT_PAT_STATUS: [PatType; 8] = [
    PatType::WriteBack,    // PAT0
    PatType::WriteThrough, // PAT1
    PatType::Uncached,     // PAT2
    PatType::Uncacheable,  // PAT3
    PatType::WriteBack,    // PAT4
    PatType::WriteThrough, // PAT5
    PatType::Uncached,     // PAT6
    PatType::Uncacheable,  // PAT7
];

/// Possible errors that can occur when working with PAT
pub(super) enum PatError {
    /// The PAT feature is not supported by the CPU.
    Unsupported,
}

/// All the possible types of memory each PAT entry can represent.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub(super) enum PatType {
    /// UC type.
    /// No caching; all accesses go directly to main memory
    Uncacheable = 0b00,
    /// WC type.
    /// Uncacheable, but writes are buffered and combined into bursts to improve performance.
    WriteCombining = 0b01,
    /// WT type.
    /// Cacheable for reads, but writes are immediately propagated to main memory
    WriteThrough = 0b100,
    /// WP type.
    /// Cacheable for reads, but writes are not allowed (read-only memory).
    WriteProtected = 0b101,
    /// WB type.
    /// Fully cacheable. Data is stored in the CPU cache and written back to main memory only when necessary
    WriteBack = 0b110,
    /// UC- type.
    /// Similar to UC but can be overridden by MTRRs to allow caching (e.g., WB or WT).
    Uncached = 0b111,
    // The rest is reserved
}

/// All the available entries in the PAT
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub(super) enum PatEntry {
    Pat0 = 0b000,
    Pat1 = 0b001,
    Pat2 = 0b010,
    Pat3 = 0b011,
    Pat4 = 0b100,
    Pat5 = 0b101,
    Pat6 = 0b110,
    Pat7 = 0b111,
}

/// Check if PAT is supported by this CPU.
pub(super) fn check_pat_support() {
    const PAT_BIT: u32 = 1 << 16;
    unsafe {
        if __cpuid(1).edx & PAT_BIT != 0 {
            // read PAT status
            // sanity_cehck it with DEFAULT_PAT_STATUS
            // change PAT_STATUS to Some(DEFAULT_PAT_STATUS)
        }
    }
}

/// Set a certain PAT entry to a specific type.
pub(super) unsafe fn set_pat_entry(entry: PatEntry, pat_type: PatType) {
    // TODO: flush affected TLB entries
    // TODO: propagate the change to all CPUs

    let mut pat: u64 = unsafe { rdmsr(IntelMsr::Ia32Pat).into() };
    pat &= !(0b111 << (entry as u8 * SHIFTING_SIZE));
    pat |= (pat_type as u64) << (entry as u8 * SHIFTING_SIZE);
    unsafe { wrmsr(IntelMsr::Ia32Pat, pat.into()) };
}

/// Get the current PAT type of a specific entry.
pub(super) fn get_pat_entry(entry: PatEntry) -> PatType {
    // If PAT is unavailable, we assume the default PAT status
    let pat: u64 = unsafe { rdmsr(IntelMsr::Ia32Pat).into() };

    ((pat >> (entry as u8 * SHIFTING_SIZE) & 0b111) as u8)
        .try_into()
        .unwrap()
}

pub(super) const fn 
/// Set the PAT entry for a page table entry.
///
/// NOTE: It is assumed that `page_table_entry` is the "mapping" entry for the page (i.e. the last
/// entry in the page table hierarchy).
/// This function is not defined for other entries.
pub(super) fn set_page_table_entry_pat(
    page_table_entry: &mut Entry,
    page_size: PageSize<X86_64>,
    pat_entry: PatEntry,
) {
    // Clear the bits
    page_table_entry.clear_flag(Entry::FLAG_PWT | Entry::FLAG_PCD);

    // Get the pat bit and clear the old one
    let pat_bit = if let PageSize::Size4KB = page_size {
        page_table_entry.clear_flag(Entry::FLAG_4KB_PAT);
        (pat_entry as usize & 0b1) << 7
    } else {
        page_table_entry.clear_flag(Entry::FLAG_1GB_PAT);
        (pat_entry as usize & 0b1) << 12
    };

    // Get the PWT and PCD bits
    let pwt_bit = (pat_entry as u8 & 0b1 << 3) as usize;
    let pcd_bit = (pat_entry as u8 & 0b1 << 4) as usize;

    page_table_entry.set_flag(pat_bit | pwt_bit | pcd_bit);
}

impl TryFrom<u8> for PatType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(PatType::Uncacheable),
            0b01 => Ok(PatType::WriteCombining),
            0b100 => Ok(PatType::WriteThrough),
            0b101 => Ok(PatType::WriteProtected),
            0b110 => Ok(PatType::WriteBack),
            0b111 => Ok(PatType::Uncached),
            // Reserved values
            _ => Err(()),
        }
    }
}

// impl SpinLockable for Option<PatStatus> {}
