use rsdp::Rsdp2;

mod madt;
mod rsdp;
mod xsdt;

#[derive(Debug)]
pub enum AcpiError {
    InvalidChecksum,
}

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
    #[inline]
    fn entry_count<T>(&self) -> usize {
        // Total length (including header) - header size gives us the total size of the entries
        let bytes_count = self.length as usize - core::mem::size_of::<SdtHeader>();
        // Should be aligned, but just making sure :)
        utils::sanity_assert!(bytes_count % core::mem::size_of::<T>() == 0);

        // Byte count to entry count
        bytes_count / core::mem::size_of::<T>()
    }

    unsafe fn validate_checksum(&self) -> Result<(), AcpiError> {
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

trait AcpiTable {
    const SIGNATURE: &'static [u8; 4];
}

pub unsafe fn init(rsdp: *const ()) -> Result<(), AcpiError> {
    utils::sanity_assert!(rsdp.is_aligned_to(align_of::<Rsdp2>()));

    let rsdp = unsafe { rsdp.cast::<Rsdp2>().as_ref().unwrap() };
    rsdp.validate_checksum()?;
    let xsdt = rsdp.get_xsdt();
    xsdt.parse_tables()?;

    log_info!("ACPI Parsed successfully");

    Ok(())
}
