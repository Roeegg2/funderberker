use super::{AcpiError, AcpiTable, SdtHeader};

// #[repr(C, packed)]
// struct EntryHeader {
//     entry_type: u8,
//     length: u8,
// }

#[repr(C, packed)]
pub(super) struct Madt {
    header: SdtHeader,
    local_apic_addr: u32,
    flags: u32,
}

impl AcpiTable for Madt {
    const SIGNATURE: &'static [u8; 4] = b"APIC";
}

impl Madt {
    pub(super) fn parse(&self) -> Result<(), AcpiError> {
        // let mut entries = unsafe {core::ptr::from_ref(self).byte_add(size_of::<Madt>()).cast::<EntryHeader>()};
        // let end = unsafe {entries.byte_add(self.header.length as usize - size_of::<Madt>())};
        //
        // while entries != end {
        //     // this should never be true, but asserting for sanity checking
        //     utils::sanity_assert!(entries < end);
        // }

        Ok(())
    }
}
