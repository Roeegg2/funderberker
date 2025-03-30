use super::{AcpiError, AcpiTable, SdtHeader};

#[derive(Debug)]
struct EntryType;

impl EntryType {
    const LOCAL_APIC: u8 = 0;
    const IO_APIC: u8 = 1;
    const IO_APIC_ISO: u8 = 2;
    const LOCAL_APIC_NMI_SRC: u8 = 3;
    const LOCAL_APIC_NMI: u8 = 4;
    const LOCAL_APIC_ADDR_OVERRIDE: u8 = 5;
    const PROCESSOR_LOCAL_X2APIC: u8 = 9;
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct EntryHeader {
    entry_type: u8,
    length: u8,
}

#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicEntry {
    header: EntryHeader,
    acpi_processor_id: u8,
    apic_id: u8,
    flags: u32,
}

#[repr(C, packed)]
#[derive(Debug)]
struct IoApicEntry {
    header: EntryHeader,
    io_apic_id: u8,
    _reserved: u8,
    io_apic_addr: u32,
    gsi_base: u32,
}

#[repr(C, packed)]
#[derive(Debug)]
struct IoApicIsoEntry {
    header: EntryHeader,
    bus_source: u8,
    irq_source: u8,
    gsi: u32,
    flags: u16,
}

#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicNmiSrcEntry {
    header: EntryHeader,
    nmi_source: u8,
    _reserved: u8,
    flags: u16,
    gsi: u32,
}

#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicNmiEntry {
    header: EntryHeader,
    acpi_processor_id: u8,
    flags: u16,
    lint: u8,
}

#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicAddrOverrideEntry {
    header: EntryHeader,
    _reserved: u16,
    // TODO: Maybe use here a PhysAddr?
    local_apic_phys_addr: u64,
}

#[repr(C, packed)]
#[derive(Debug)]
struct ProcessorLocalx2ApicEntry {
    header: EntryHeader,
    _reserved: u16,
    processor_x2apic_id: u32,
    flags: u32,
    acpi_id: u32,
}

#[repr(C, packed)]
#[derive(Debug)]
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
        let mut entries = unsafe {core::ptr::from_ref(self).add(1).cast::<EntryHeader>()};
        let end_addr = core::ptr::from_ref(self).addr() + self.header.length as usize;

        while entries.addr() != end_addr {
            utils::sanity_assert!(entries.addr() < end_addr);

            let entry_type = unsafe {entries.read().entry_type};
            match entry_type {
                EntryType::IO_APIC => {
                    // TODO: Move this to some global structure
                    let entry = unsafe {entries.cast::<IoApicEntry>().as_ref().unwrap()};
                    let ioapic = unsafe {crate::ioapic::IoApic::new(entry.io_apic_addr, entry.gsi_base)};
                    println!("{:?}", ioapic);
                },
                _ => (),
            }

            let entry_length = unsafe {entries.read().length as usize};
            entries = unsafe {entries.byte_add(entry_length)};
        }

        Ok(())
    }
}
