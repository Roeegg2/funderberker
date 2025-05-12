//! Parser for the RSDP table

use super::{AcpiError, SdtHeader, xsdt::Xsdt};
use crate::{arch::{x86_64::paging::{self, Entry}, BASIC_PAGE_SIZE}, mem::{vmm::map_page, PhysAddr}};
use core::ptr;

/// The RSDP (a pointer to the XSDT)
#[repr(C, packed)]
struct Rsdp {
    /// The signature of the RSDP
    signature: [u8; 8],
    /// The checksum to compare of the RSDP
    checksum: u8,
    /// The OEM ID
    oem_id: [u8; 6],
    /// The OEM table ID
    revision: u8,
    /// The OEM revision
    rsdt_address: u32,
}

/// The RSDP2 (the extended version of the RSDP)
///
/// This is an extension to the classic `Rsdp`
#[repr(C, packed)]
pub(super) struct Rsdp2 {
    old: Rsdp,

    // Part 2 (the extension)
    /// The length of the RSDP2
    length: u32,
    /// The revision of the RSDP2
    xsdt_address: u64,
    /// The extended checksum of the RSDP2
    extended_checksum: u8,
    /// Reserved field
    _reserved: [u8; 3],
}

impl Rsdp2 {
    /// Validate the checksum of the RSDP2
    pub(super) fn validate_checksum(&self) -> Result<(), AcpiError> {
        // RSDP2 checksum is calculated for the original fields, and the extended fields separately
        // Calculate checksum for the original Rsdp
        {
            let mut sum: usize = 0;
            let ptr = ptr::from_ref(self).cast::<u8>();
            for i in 0..size_of::<Rsdp>() {
                sum += unsafe { *(ptr.add(i)) } as usize;
            }

            if sum & 0xff != 0 {
                return Err(AcpiError::InvalidChecksum);
            }
        }
        // Calculate checksum for the new fields
        {
            let mut sum: usize = 0;
            let ptr = unsafe { ptr::from_ref(self).cast::<u8>().byte_add(size_of::<Rsdp>()) };
            for i in 0..(size_of::<Rsdp2>() - size_of::<Rsdp>()) {
                sum += unsafe { *(ptr.add(i)) } as usize;
            }

            if sum & 0xff != 0 {
                return Err(AcpiError::InvalidChecksum);
            }
        }

        Ok(())
    }

    /// Get a pointer to the XSDT
    #[inline]
    pub(super) fn get_xsdt(&self) -> &Xsdt {
        let ptr: *const SdtHeader = unsafe {
            let addr = PhysAddr(self.xsdt_address as usize);
            let diff = addr.0 % BASIC_PAGE_SIZE;
            (map_page(addr - diff, Entry::FLAGS_NONE) + diff).into()
        };

        utils::sanity_assert!(ptr.is_aligned_to(align_of::<Xsdt>()));

        unsafe { ptr.cast::<Xsdt>().as_ref().unwrap() }
    }
}
