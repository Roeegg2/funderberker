//! Parser for the XSDT table

use logger::*;
use core::ptr::from_ref;
use super::{AcpiError, AcpiTable, SdtHeader, madt::Madt, mcfg::Mcfg, hpet::Hpet};
use kernel::{
    arch::{BASIC_PAGE_SIZE, x86_64::paging::Entry},
    mem::vmm::map_page,
};
use utils::mem::PhysAddr;

/// The XSDT
#[derive(Debug)]
#[repr(C)]
pub(super) struct Xsdt {
    /// The SDT header
    header: SdtHeader,
}

impl Xsdt {
    /// Get an iterator over the entries in the XSDT
    #[inline]
    fn iter(&self) -> Iter {
        let count = self.header.entry_count::<PhysAddr>();
        let ptr: *const PhysAddr = unsafe { from_ref(self).add(1).cast::<PhysAddr>() };

        Iter { ptr, count }
    }

    /// Parse the ACPI tables in the XSDT
    pub(super) fn parse_tables(&self) -> Result<(), AcpiError> {
        self.header.validate_checksum()?;

        for entry in self.iter() {
            let signature = &unsafe { (*entry).signature };
            match signature {
                Madt::SIGNATURE => {
                    let madt = unsafe { entry.cast::<Madt>().as_ref().unwrap() };
                    madt.parse()?;
                }
                Hpet::SIGNATURE => {
                    let hpet = unsafe { entry.cast::<Hpet>().as_ref().unwrap() };
                    hpet.setup_hpet()?;
                }
                Mcfg::SIGNATURE => {
                    let mcfg = unsafe { entry.cast::<Mcfg>().as_ref().unwrap() };
                    mcfg.parse()?;
                }
                _ => continue,
                // _ => {
                //     log_warn!(
                //         "ACPI: Unhandled table: {:?}",
                //         core::str::from_utf8(signature)
                //     );
                //     continue;
                // }
            }

            log_info!("ACPI: Parsed table: {:?}", core::str::from_utf8(signature));
        }

        Ok(())
    }
}

/// An iterator over the entries in the XSDT
struct Iter {
    ptr: *const PhysAddr,
    count: usize,
}

impl Iterator for Iter {
    type Item = *const SdtHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }

        let ptr: *const SdtHeader = unsafe {
            let addr = self.ptr.read_unaligned();
            let diff = addr.0 % BASIC_PAGE_SIZE;
            (map_page(addr - diff, Entry::FLAGS_NONE) + diff).into()
        };

        self.ptr = unsafe { self.ptr.add(1) };
        self.count -= 1;

        Some(ptr)
    }
}
