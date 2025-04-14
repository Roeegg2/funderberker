use crate::mem::PhysAddr;

use super::{AcpiError, SdtHeader, xsdt::Xsdt};

#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
pub(super) struct Rsdp2 {
    old: Rsdp,

    // Part 2 (the extension)
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

impl Rsdp2 {
    pub(super) fn validate_checksum(&self) -> Result<(), AcpiError> {
        // RSDP2 checksum is calculated for the original fields, and the extended fields separately
        let sum1 = unsafe {
            core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), size_of::<Rsdp>())
        }
        .iter()
        .fold(0, |acc, &x| acc + x as usize);
        let sum2 = unsafe {
            core::slice::from_raw_parts(
                core::ptr::from_ref(self)
                    .cast::<u8>()
                    .byte_add(size_of::<Rsdp>()),
                size_of::<Rsdp2>() - size_of::<Rsdp>(),
            )
        }
        .iter()
        .fold(0, |acc, &x| acc + x as usize);

        // Make sure the sum casted to a u8 is 0
        if sum1 % 0x100 != 0 || sum2 % 0x100 != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }

    #[inline]
    pub(super) fn get_xsdt(&self) -> &Xsdt {
        let addr = self.xsdt_address;

        let ptr: *const SdtHeader = PhysAddr(addr as usize).add_hhdm_offset().into();
        utils::sanity_assert!(ptr.is_aligned_to(align_of::<Xsdt>()));

        unsafe { ptr.cast::<Xsdt>().as_ref().unwrap() }
    }
}
