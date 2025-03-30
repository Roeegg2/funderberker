use super::{AcpiError, AcpiTable, SdtHeader};

#[derive(Debug)]
pub(super) enum MadtError {
    InvalidEntry,
    InvalidPointer,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct EntryHeader {
    entry_type: u8,
    length: u8,
}

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

#[derive(Debug)]
enum MadtEntry {
    LocalApic(*const LocalApicEntry),
    IoApic(*const IoApicEntry),
    IoApicIso(*const IoApicIsoEntry),
    LocalApicNmiSrc(*const LocalApicNmiSrcEntry),
    LocalApicNmi(*const LocalApicNmiEntry),
    LocalApicAddrOverride(*const LocalApicAddrOverrideEntry),
    ProcessorLocalx2Apic(*const ProcessorLocalx2ApicEntry),
}

impl MadtEntry {
    fn parse(self) {
        match self {
            MadtEntry::LocalApic(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::IoApic(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::IoApicIso(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::LocalApicNmiSrc(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::LocalApicNmi(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::LocalApicAddrOverride(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
            MadtEntry::ProcessorLocalx2Apic(entry) => {
                println!("{:?}", unsafe {entry.as_ref()});
            },
        }
    }
}

impl<'a> TryFrom<*const EntryHeader> for MadtEntry {
    type Error = MadtError;

    fn try_from(entry: *const EntryHeader) -> Result<Self, Self::Error> {
        let entry_type = unsafe {entry.as_ref().ok_or(MadtError::InvalidPointer)?.entry_type};

        match entry_type {
            EntryType::LOCAL_APIC => Ok(MadtEntry::LocalApic(entry.cast())),
            EntryType::IO_APIC => Ok(MadtEntry::IoApic(entry.cast())),
            EntryType::IO_APIC_ISO => Ok(MadtEntry::IoApicIso(entry.cast())),
            EntryType::LOCAL_APIC_NMI_SRC => Ok(MadtEntry::LocalApicNmiSrc(entry.cast())),
            EntryType::LOCAL_APIC_NMI => Ok(MadtEntry::LocalApicNmi(entry.cast())),
            EntryType::LOCAL_APIC_ADDR_OVERRIDE => Ok(MadtEntry::LocalApicAddrOverride(entry.cast())),
            EntryType::PROCESSOR_LOCAL_X2APIC => Ok(MadtEntry::ProcessorLocalx2Apic(entry.cast())),
            _ => Err(MadtError::InvalidEntry),
        }
    }
}

utils::sum_fields!(EntryHeader { entry_type, length });
utils::sum_fields!(LocalApicEntry { acpi_processor_id, apic_id, flags });
utils::sum_fields!(IoApicEntry { io_apic_id, _reserved, io_apic_addr, gsi_base });
utils::sum_fields!(IoApicIsoEntry { bus_source, irq_source, gsi, flags });
utils::sum_fields!(LocalApicNmiSrcEntry { nmi_source, _reserved, flags, gsi });
utils::sum_fields!(LocalApicNmiEntry { acpi_processor_id, flags, lint });
utils::sum_fields!(LocalApicAddrOverrideEntry { _reserved, local_apic_phys_addr });
utils::sum_fields!(ProcessorLocalx2ApicEntry { _reserved, processor_x2apic_id, flags, acpi_id });

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

    first_record: EntryHeader,
}

impl Madt {
    fn iter(&self) -> Iter {
        Iter {
            current: &self.first_record,
            end: core::ptr::from_ref(self).addr() + self.header.length as usize,
            // end: ((self as *const Madt) as usize + self.header.length as usize) as *const EntryHeader,
        }
    }

    pub(super) fn parse(&self) -> Result<(), MadtError> {
        // println!("{:?}", self.header);
        for entry in self.iter() {
            let entry: MadtEntry = entry.try_into()?;
            entry.parse();
        }
        
        Ok(())
    }
}

impl AcpiTable for Madt {
    const SIGNATURE: &'static [u8; 4] = b"APIC";

    fn validate(&self) -> Result<(), AcpiError> {
        let sum = unsafe {core::slice::from_raw_parts(core::ptr::from_ref(self).cast::<u8>(), self.header.length as usize)}.iter().sum::<u8>() as usize;
        if sum != 0 {
            return Err(AcpiError::InvalidChecksum);
        }

        Ok(())
    }
}

struct Iter {
    current: *const EntryHeader,
    end: usize,
}

impl Iterator for Iter {
    type Item = *const EntryHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.addr() >= self.end {
            return None;
        }

        let entry = self.current;
        self.current = unsafe { self.current.byte_add((*self.current).length as usize) };
        Some(entry)
    }
}

