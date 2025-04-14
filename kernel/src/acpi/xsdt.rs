use crate::mem::PhysAddr;

use super::{AcpiError, AcpiTable, SdtHeader, madt::Madt};

#[repr(C)]
pub(super) struct Xsdt {
    header: SdtHeader,
}

impl Xsdt {
    #[inline]
    fn iter(&self) -> Iter {
        let count = self.header.entry_count::<PhysAddr>();
        let ptr: *const PhysAddr = unsafe { core::ptr::from_ref(self).add(1).cast::<PhysAddr>() };

        Iter { ptr, count }
    }

    pub(super) fn parse_tables(&self) -> Result<(), AcpiError> {
        unsafe { self.header.validate_checksum()? };

        for entry in self.iter() {
            let signature = &unsafe { (*entry).signature };
            match signature {
                Madt::SIGNATURE => {
                    let madt = unsafe { entry.cast::<Madt>().as_ref().unwrap() };
                    madt.parse()?;
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

        let ptr: *const SdtHeader = unsafe { self.ptr.read_unaligned() }
            .add_hhdm_offset()
            .into();

        self.ptr = unsafe { self.ptr.add(1) };
        self.count -= 1;

        Some(ptr)
    }
}
