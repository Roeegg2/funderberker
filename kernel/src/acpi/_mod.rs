//! Basic ACPI table parsing

mod xsdt;
mod madt;

use xsdt::Xsdt;
use madt::Madt;

use crate::mem::PhysAddr;

/// Calculate the checksum of a table and return an error if it's invalid
macro_rules! checksums_8bit {
    ($($sum:expr),*) => {
        if $($sum == 0)||* {
            return Err(AcpiError::InvalidChecksum);
        }
    };
}

pub(self) use checksums_8bit;

/// Errors that we might encounter whilst parsing the ACPI tables.
#[derive(Debug, Clone, Copy)]
enum AcpiError {
    /// The checksum of the table is invalid.
    InvalidChecksum,
}

// NOTE: We are not going to support the old RSDP, since it's deprecated and we're not going to
// support old hardware.
/// Version 2.0 and above of the Root System Description Pointer (RSDP) structure.
#[repr(C, packed)]
#[derive(Debug)]
pub struct Rsdp2 {
    // Part 1
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,

    // Part 2 (the extension)
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

impl Rsdp2 {
    /// The magic revision number that we want to see in the RSDP (marks ACPI 2.0+)
    const WANTED_REVISION_MAGIC: u8 = 2;

    /// Get a reference to the XSDT table
    #[inline]
    pub unsafe fn get_xsdt(&self) -> &Xsdt {
        let ptr: *const Xsdt = PhysAddr(self.xsdt_address as usize).add_hhdm_offset().into();
        unsafe {ptr.as_ref().unwrap()}
    }

    /// Make sure the RSDP isn't corrupted by calculating and comparing the checksum
    #[inline]
    fn validate(&self) -> Result<(), AcpiError> {
        const PART_1_SIZE: usize = 8 + 1 + 6 + 1 + 4;
        let sum1 = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), PART_1_SIZE)}.iter().sum::<u8>() as usize;
        let sum2 = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>().add(PART_1_SIZE), size_of::<Rsdp2>() - PART_1_SIZE)}.iter().sum::<u8>() as usize;

        if sum1 & 0xff != 0 || sum2 & 0xff != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }
}

/// The header of an ACPI table (except for the RSDP, which has a modified one)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub(self) struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

impl SdtHeader {
    /// Calculate the sum of all fields of the header
    #[inline]
    fn sum(&self) -> usize {
        self.signature.iter().sum::<u8>() as usize + self.length as usize + self.revision as usize + self.checksum as usize + self.oem_id.iter().sum::<u8>() as usize + self.oem_table_id.iter().sum::<u8>() as usize + self.oem_revision as usize + self.creator_id as usize + self.creator_revision as usize
    }
}

/// Common ACPI table functionality
pub(self) trait AcpiTable {
    /// The signature of the table
    const SIGNATURE: &'static [u8; 4];

    /// Make sure the table data isn't corrupted by calculating and comparing the checksum
    fn validate(&self) -> Result<(), AcpiError>;
}

// TODO: Generalize this?
/// An iterator over the entries in an ACPI table
#[derive(Debug)]
pub(self) struct AcpiTableIter<'a> {
    entries: &'a [PhysAddr],
    index: usize,
}

impl<'a> Iterator for AcpiTableIter<'a> {
    type Item = *const SdtHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.entries.len() {
            let entry = self.entries[self.index];
            self.index += 1;
            Some(entry.add_hhdm_offset().into())
        } else {
            None
        }
    }
}

pub(self) fn do_checksum(data: &[u8]) -> Result<(), AcpiError> {
    if data.iter().sum::<u8>() == 0 {
        return Err(AcpiError::InvalidChecksum);
    }

    Ok(())
}


/// Initialize the ACPI subsystem.
pub unsafe fn init(rsdp: &Rsdp2) {
    // Should always be true since we are booting with UEFI, but sanity checking for this anyway
    rsdp.validate().unwrap();
    assert!(rsdp.revision == Rsdp2::WANTED_REVISION_MAGIC);

    let xsdt: &Xsdt = unsafe {rsdp.get_xsdt()};
    xsdt.validate().unwrap();

    println!("XSDT: {:?}", xsdt);

    println!("DIFF {:?}", core::ptr::from_ref(xsdt).addr() - core::ptr::from_ref(&(xsdt.entries)).addr());

    let madt = unsafe {xsdt.iter().find(|&entry| {
        println!("{:?}", entry);
        (*entry).signature == *Madt::SIGNATURE
    }).unwrap().cast::<Madt>().as_ref().unwrap()};

    println!("MADT");

    madt.parse().unwrap();

    println!("MADT: {:?}", madt);

}
