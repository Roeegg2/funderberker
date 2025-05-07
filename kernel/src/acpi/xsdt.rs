use core::ptr::from_ref;

use super::{AcpiError, AcpiTable, SdtHeader, madt::Madt};
#[cfg(all(target_arch = "x86_64", feature = "hpet"))]
use crate::acpi::hpet::Hpet;
use crate::mem::PhysAddr;

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
        unsafe { self.header.validate_checksum()? };

        for entry in self.iter() {
            let signature = &unsafe { (*entry).signature };
            match signature {
                Madt::SIGNATURE => {
                    let madt = unsafe { entry.cast::<Madt>().as_ref().unwrap() };
                    madt.parse()?;
                }
                #[cfg(all(target_arch = "x86_64", feature = "hpet"))]
                Hpet::SIGNATURE => {
                    let hpet = unsafe { entry.cast::<Hpet>().as_ref().unwrap() };
                    hpet.setup_hpet()?;
                }
                _ => {
                    log_warn!(
                        "ACPI: Unhandled table: {:?}",
                        core::str::from_utf8(signature)
                    );
                    continue;
                }
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

        // let ptr: *const SdtHeader = unsafe {
        //     map_page(self.ptr.read_unaligned(), Entry::FLAG_RW)
        // }.into();
        let ptr: *const SdtHeader = unsafe { self.ptr.read_unaligned().add_hhdm_offset().into() };

        self.ptr = unsafe { self.ptr.add(1) };
        self.count -= 1;

        Some(ptr)
    }
}
