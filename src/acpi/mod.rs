// NOTE: Only supporting ACPI version 2+. no x86_64 CPU is using ACPI 1.

use crate::uefi::{Guid, SystemTable};

#[derive(Debug)]
#[allow(unused)]
pub enum AcpiError {
    VendorTableNotFound,
    RsdpNull,
    BadRsdpSignature,
    FoundAcpiVersion1,
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    _rsdt_address: u32,

    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

//pub fn parse_acpi(system_table: &SystemTable) {
//    let _rsdp = get_rsdp(system_table).unwrap();
//}

//fn get_rsdp(system_table: &SystemTable) -> Result<(), AcpiError> {
//    const ACPI_VERSION_MAGIC: u8 = 2;
//    const ACPI_20_GUID: Guid = (
//        0x8868e871, 0xe4f1, 0x11d3, 0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81,
//    );
//
//    // Get pointer, convert to *const Rsdp and then to &Rsdp
//    let rsdp = unsafe {
//        (system_table
//            .get_vendor_table(&ACPI_20_GUID)
//            .ok_or(AcpiError::VendorTableNotFound)? as *const Rsdp)
//            .as_ref()
//    }
//    .ok_or(AcpiError::RsdpNull)?;
//
//    // Check RSDP signature
//    if &rsdp.signature != b"RSD PTR " {
//        return Err(AcpiError::BadRsdpSignature);
//    }
//
//    // Sanity checking for version 2+
//    if rsdp.revision != ACPI_VERSION_MAGIC {
//        return Err(AcpiError::FoundAcpiVersion1);
//    }
//
//    Ok(())
//}
