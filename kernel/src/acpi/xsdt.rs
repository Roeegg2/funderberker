
use crate::mem::PhysAddr;

use super::{madt::Madt, AcpiError, AcpiTable, SdtHeader};

#[repr(C, packed)]
pub(super) struct Xsdt {
    header: SdtHeader,
}

impl Xsdt {
    #[inline]
    const fn get_table_count(&self) -> usize {
        // asserting jusdt for sanity checking
        debug_assert!(self.header.length as usize % core::mem::size_of::<PhysAddr>() == 0);
        (self.header.length as usize - core::mem::size_of::<SdtHeader>()) / core::mem::size_of::<PhysAddr>()
    } 

    pub(super) fn parse_tables(&self) -> Result<(), AcpiError> {
        let entries: &[PhysAddr] = unsafe {
            let count = self.get_table_count();
            core::slice::from_raw_parts(core::ptr::from_ref(self).byte_add(size_of::<Xsdt>()).cast::<PhysAddr>(), count)
        };

        for entry_addr in entries {
            let entry_ptr: *const SdtHeader = entry_addr.add_hhdm_offset().into();
            let entry = unsafe {entry_ptr.as_ref_unchecked()};

            match &entry.signature {
                Madt::SIGNATURE => {
                    let madt = unsafe {entry_ptr.cast::<Madt>().as_ref_unchecked()};
                    madt.parse()?;
                    log!("ACPI: Parsed MADT successfully!");
                },
                _ => {
                    log!("ACPI: Unhandled table: {:?}", core::str::from_utf8(&(entry.signature)));
                },
            }
        }

        Ok(())
    }
}
