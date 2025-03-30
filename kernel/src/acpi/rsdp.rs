
use crate::mem::PhysAddr;

use super::{xsdt::Xsdt, AcpiError, SdtHeader};

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
        let sum1 = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), size_of::<Rsdp>())}.iter().sum::<u8>() as usize;
        let sum2 = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>().byte_add(size_of::<Rsdp>()), size_of::<Rsdp2>() - size_of::<Rsdp>())}.iter().sum::<u8>() as usize;

        if sum1 & 0xff != 0 || sum2 & 0xff != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }

    #[inline]
    pub(super) fn get_xsdt(&self) -> Result<&Xsdt, AcpiError> {
        let ptr: *const SdtHeader = PhysAddr(self.xsdt_address as usize).add_hhdm_offset().into();
        unsafe {
            let header = ptr.as_ref().unwrap();
            header.validate_checksum()?;
        
            Ok(ptr.cast::<Xsdt>().as_ref().unwrap())
        }
    }
}
