//! Parsing of the `MCFG` ACPI table

use core::{ptr::from_ref, slice::from_raw_parts};

use drivers::bus::pcie::{ConfigSpace, PcieManager};
use utils::sanity_assert;

use super::{AcpiError, AcpiTable, SdtHeader};

/// The MCFG table
#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct Mcfg {
    /// The common SDT table header
    header: SdtHeader,
    /// Reserved
    _reserved: u64,
}

impl Mcfg {
    /// The MCFG doesn't explicitly list the number of entries, so we need to manually calculate it
    #[inline]
    fn determine_entries_count(&self) -> usize {
        // The total size of the MCFG table minus the header size gives us the size of the entries
        let total_size = self.header.length as usize - size_of::<Mcfg>();
        sanity_assert!(total_size % size_of::<ConfigSpace>() == 0);

        total_size / size_of::<ConfigSpace>()
    }

    /// Parse the entries in the MCFG
    pub(super) fn parse(&self) -> Result<(), AcpiError> {
        unsafe {
            // we need to do this trick since Mcfg is packed, so direct access of `Mcfg.header` is not aligned
            let header_ref = from_ref(self).cast::<SdtHeader>().as_ref().unwrap();
            header_ref.validate_checksum()?;
        }

        let entries = {
            let count = self.determine_entries_count();
            let entries_ptr = unsafe { from_ref(self).add(1).cast::<ConfigSpace>() };

            unsafe { from_raw_parts(entries_ptr, count) }
        };

        PcieManager::init(entries).unwrap();

        Ok(())
    }
}

impl AcpiTable for Mcfg {
    const SIGNATURE: &'static [u8; 4] = b"MCFG";
}
