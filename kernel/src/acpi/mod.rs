//! Basic ACPI table parsing

mod xsdt;
mod madt;

use xsdt::Xsdt;
//use madt::Madt;

use crate::mem::PhysAddr;

/// Calculate the checksum of a table and return an error if it's invalid
macro_rules! checksums {
    ($($sum:expr),*) => {
        if $(($sum & 0xff == 0))||* {
            return Err(AcpiError::InvalidChecksum);
        }
    };
}

pub(self) use checksums;

/// Errors that we might encounter whilst parsing the ACPI tables.
#[derive(Debug, Clone, Copy)]
enum AcpiError {
    /// The checksum of the table is invalid.
    InvalidChecksum,
}

// NOTE: We are not going to support the old RSDP, since it's deprecated and we're not going to
// support old hardware.
/// Version 2.0 and above of the Root System Description Pointer (RSDP) structure.
#[repr(C)]
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
    reserved: [u8; 3],
}

impl Rsdp2 {
    /// The magic revision number that we want to see in the RSDP (marks ACPI 2.0+)
    const WANTED_REVISION_MAGIC: u8 = 2;

    /// Get a reference to the XSDT table
    #[inline]
    pub unsafe fn get_xsdt(&self) -> &Xsdt {
        let ptr: *const Xsdt = PhysAddr(self.xsdt_address as usize).add_hhdm_offset().into();
        println!("XSDT: {:?}", ptr);
        unsafe {ptr.as_ref().unwrap()}
    }

    /// Make sure the RSDP isn't corrupted by calculating and comparing the checksum
    #[inline]
    fn validate(&self) -> Result<(), AcpiError> {
        let sum1: usize = self.signature.iter().sum::<u8>() as usize + self.checksum as usize + self.oem_id.iter().sum::<u8>() as usize + self.revision as usize + self.rsdt_address as usize;
        let sum2: usize = self.length as usize + self.xsdt_address as usize + self.extended_checksum as usize + self.reserved.iter().sum::<u8>() as usize;
        checksums!(sum1, sum2);

        Ok(())
    }
}

/// The header of an ACPI table (except for the RSDP, which has a modified one)
#[repr(C)]
#[derive(Debug)]
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
    /// Make sure the SDT header isn't corrupted by calculating and comparing the checksum
    fn validate(&self) -> Result<(), AcpiError> {
        let sum: usize = self.signature.iter().sum::<u8>() as usize + self.length as usize + self.revision as usize + self.checksum as usize + self.oem_id.iter().sum::<u8>() as usize + self.oem_table_id.iter().sum::<u8>() as usize + self.oem_revision as usize + self.creator_id as usize + self.creator_revision as usize;
        checksums!(sum);

        Ok(())
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
pub(self) struct AcpiTableIter<'a> {
    entries: &'a [*const SdtHeader],
    index: usize,
}

impl<'a> Iterator for AcpiTableIter<'a> {
    type Item = *const SdtHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.entries.len() {
            let entry = self.entries[self.index];
            self.index += 1;
            Some(entry)
        } else {
            None
        }
    }
}


/// Initialize the ACPI subsystem.
pub unsafe fn init(rsdp: &Rsdp2) {
    // Should always be true since we are booting with UEFI, but sanity checking for this anyway
    rsdp.validate().unwrap();
    assert!(rsdp.revision == Rsdp2::WANTED_REVISION_MAGIC);

    let xsdt: &Xsdt = unsafe {rsdp.get_xsdt()};
    xsdt.validate().unwrap();

    println!("XSDT: {:?}", xsdt);

    //let madt = unsafe {xsdt.iter().find(|&entry| {
    //    (*entry).signature == *Madt::SIGNATURE
    //}).unwrap().as_ref().unwrap()};

    //println!("MADT: {:?}", madt);

}
