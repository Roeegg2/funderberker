//! ACPI table parser

use core::{ptr::from_ref, slice::from_raw_parts};
use kernel::{
    arch::{BASIC_PAGE_SIZE, x86_64::X86_64},
    mem::paging::{Flags, PageSize, PagingManager},
};
use rsdp::Rsdp2;
use utils::{mem::PhysAddr, sanity_assert};

mod hpet;
mod madt;
pub mod mcfg;
mod rsdp;
mod xsdt;

/// Errors that can occur while parsing ACPI tables
#[derive(Debug)]
pub enum AcpiError {
    /// The checksum of the table is invalid
    InvalidChecksum,
}

/// The ACPI GAS (Generic Address Structure)
#[repr(C, packed)]
#[derive(Debug)]
struct Gas {
    space_id: u8,
    register_bit_width: u8,
    register_bit_offset: u8,
    _reserved: u8,
    addr: u64,
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
        let bytes_count = self.length as usize - size_of::<SdtHeader>();
        // Should be aligned, but just making sure :)
        sanity_assert!(bytes_count % size_of::<T>() == 0);

        // Byte count to entry count
        bytes_count / core::mem::size_of::<T>()
    }

    /// Validate the checksum of the table
    fn validate_checksum(&self) -> Result<(), AcpiError> {
        let sum = unsafe { from_raw_parts(from_ref(self).cast::<u8>(), self.length as usize) }
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
    sanity_assert!(rsdp_addr.0 % align_of::<Rsdp2>() == 0);

    let rsdp = unsafe {
        let diff = rsdp_addr.0 % BASIC_PAGE_SIZE.size();
        let ptr: *const Rsdp2 =
            X86_64::map_pages(rsdp_addr - diff, 1, Flags::new(), PageSize::size_4kb())
                .unwrap()
                .byte_add(diff)
                .cast();

        ptr.as_ref().unwrap()
    };

    rsdp.validate_checksum()?;
    let xsdt = rsdp.get_xsdt();
    xsdt.parse_tables()?;

    logger::info!("ACPI: All tables parsed successfully");

    Ok(())
}
