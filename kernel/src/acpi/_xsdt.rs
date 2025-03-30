
use super::*;

#[repr(C, packed)]
#[derive(Debug)]
pub(super) struct Xsdt {
    /// Common SDT header
    header: SdtHeader,
    /// Array of pointers to other SDTs
    pub entries: PhysAddr,
}

impl Xsdt {
    #[inline]
    const fn get_table_count(&self) -> usize {
        (self.header.length as usize - core::mem::size_of::<SdtHeader>()) / core::mem::size_of::<*const SdtHeader>()
    }

    pub fn iter(&self) -> AcpiTableIter {
        AcpiTableIter {
            entries: unsafe {core::slice::from_raw_parts(
                         self.entries.add_hhdm_offset().into(), self.get_table_count())},
            index: 0,
        }
    }
}

impl AcpiTable for Xsdt {
    const SIGNATURE: &'static [u8; 4] = b"XSDT";

    fn validate(&self) -> Result<(), AcpiError> {
        // Calculate the sum of the header + all the pointers
        // let sum = self.iter().fold(0, |acc, x| acc + x.addr()) + self.header.sum();
        // println!("thats the sum: {sum}");
        // checksums!(sum);

        let sum = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), self.header.length as usize)}.iter().sum::<u8>() as usize;
        if sum != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }
}
