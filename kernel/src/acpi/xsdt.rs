
use crate::mem::PhysAddr;

use super::{madt::Madt, AcpiError, AcpiTable, SdtHeader};

#[repr(C, packed)]
pub(super) struct Xsdt {
    header: SdtHeader,
}

impl Xsdt {
    #[inline]
    fn get_table_count(&self) -> usize {
        // Total length (including header) - header size gives us the total size of the entries
        let bytes_count = self.header.length as usize - core::mem::size_of::<SdtHeader>();
        // Should be aligned, but just making sure :)
        utils::sanity_assert!(bytes_count % core::mem::size_of::<PhysAddr>() == 0);

        // Byte count to entry count 
        bytes_count / core::mem::size_of::<PhysAddr>()
    } 

    pub(super) fn parse_tables(&self) -> Result<(), AcpiError> {
        unsafe {self.header.validate_checksum()?};

        let entries: *const PhysAddr = unsafe {
            core::ptr::from_ref(self).add(1).cast::<PhysAddr>()
        };

        let count = self.get_table_count();
        for i in 0..count {
            // SAFETY: Specification dictates that pointers to the SDTs are 4 byte aligned, so we
            // need to use `read_unaligned` to read the pointers
            let ptr: *const SdtHeader = unsafe {entries.add(i).read_unaligned().add_hhdm_offset().into()};
            assert!(ptr.is_aligned_to(align_of::<Madt>()));

            let signature = &unsafe {(*ptr).signature};
            match signature {
                Madt::SIGNATURE => {
                    let madt = unsafe {ptr.cast::<Madt>().as_ref().unwrap()};
                    madt.parse()?;
                },
                _ => {
                    log!("ACPI: Unhandled table: {:?}", core::str::from_utf8(signature));
                    continue;
                },
            }

            log!("ACPI: Parsed table: {:?}", core::str::from_utf8(signature));
        }

        Ok(())
    }
}
