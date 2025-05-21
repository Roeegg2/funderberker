//! ACPI table parser

use rsdp::Rsdp2;

use crate::{
    arch::{BASIC_PAGE_SIZE, x86_64::paging::Entry},
    mem::{PhysAddr, vmm::map_page},
};

#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
mod hpet;
mod madt;
mod rsdp;
mod xsdt;

/// Errors that can occur while parsing ACPI tables
#[derive(Debug)]
pub enum AcpiError {
    /// The checksum of the table is invalid
    InvalidChecksum,
}

/// The header that comes before (almost) all ACPI table
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SdtHeader {
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
    /// Get the entry count for tables with fixed size entries
    #[inline]
    fn entry_count<T>(&self) -> usize {
        // Total length (including header) - header size gives us the total size of the entries
        let bytes_count = self.length as usize - core::mem::size_of::<SdtHeader>();
        // Should be aligned, but just making sure :)
        utils::sanity_assert!(bytes_count % core::mem::size_of::<T>() == 0);

        // Byte count to entry count
        bytes_count / core::mem::size_of::<T>()
    }

    /// Validate the checksum of the table
    fn validate_checksum(&self) -> Result<(), AcpiError> {
        let sum = unsafe {
            core::slice::from_raw_parts(
                core::ptr::from_ref(self).cast::<u8>(),
                self.length as usize,
            )
        }
        .iter()
        .fold(0, |acc, &x| acc + x as usize);

        if sum % 0x100 != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }
}

/// A trait that all ACPI tables should implement, in order for the parser to be able to do it's
/// job
trait AcpiTable {
    /// The signature of the table
    const SIGNATURE: &'static [u8; 4];
}

/// Initialize the ACPI subsystem
pub unsafe fn init(rsdp_addr: PhysAddr) -> Result<(), AcpiError> {
    utils::sanity_assert!(rsdp_addr.0 % align_of::<Rsdp2>() == 0);

    let rsdp = unsafe {
        let diff = rsdp_addr.0 % BASIC_PAGE_SIZE;
        let ptr: *const Rsdp2 = (map_page(rsdp_addr - diff, 0) + diff).into();
        ptr.as_ref().unwrap()
    };

    rsdp.validate_checksum()?;
    let xsdt = rsdp.get_xsdt();
    xsdt.parse_tables()?;

    log_info!("ACPI: All tables parsed successfully");

    Ok(())
}
