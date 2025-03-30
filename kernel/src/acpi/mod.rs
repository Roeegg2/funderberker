use rsdp::Rsdp2;

mod rsdp;
mod xsdt;
mod madt;

#[derive(Debug)]
pub enum AcpiError {
    InvalidChecksum,
}

#[repr(C, packed)]
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
    pub(self) unsafe fn validate_checksum(&self) -> Result<(), AcpiError> {
        let sum = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), self.length as usize)}.iter().sum::<u8>() as usize;
        if sum != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }
}

pub(self) trait AcpiTable {
    const SIGNATURE: &'static [u8; 4];
}

pub unsafe fn init(rsdp: *const ()) -> Result<(), AcpiError> {
    let rsdp = unsafe {rsdp.cast::<Rsdp2>().as_ref().unwrap()};
    rsdp.validate_checksum()?;
    let xsdt = rsdp.get_xsdt()?;
    xsdt.parse_tables()?;

    log!("ACPI Parsed successfully");

    Ok(())
}
