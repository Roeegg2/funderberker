
use super::*;

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
    const fn sum_fields(&self) -> usize {
        match self {
            MadtEntry::LocalApic(entry) => unsafe {entry.as_ref().unwrap().sum_fields()},
            MadtEntry::IoApic(entry) => entry.sum_fields(),
            MadtEntry::IoApicIso(entry) => entry.sum_fields(),
            MadtEntry::LocalApicNmiSrc(entry) => entry.sum_fields(),
            MadtEntry::LocalApicNmi(entry) => entry.sum_fields(),
            MadtEntry::LocalApicAddrOverride(entry) => entry.sum_fields(),
            MadtEntry::ProcessorLocalx2Apic(entry) => entry.sum_fields(),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
struct EntryHeader {
    entry_type: u8,
    length: u8,
}

impl EntryHeader {
    fn to_entry(&self) -> Option<MadtEntry> {
        match self.entry_type {
            EntryType::LOCAL_APIC => Some(MadtEntry::LocalApic(core::ptr::from_ref(self).cast::<LocalApicEntry>())),
            EntryType::IO_APIC => Some(MadtEntry::IoApic(core::ptr::from_ref(self).cast::<IoApicEntry>())),
            EntryType::IO_APIC_ISO => Some(MadtEntry::IoApicIso(core::ptr::from_ref(self).cast::<IoApicIsoEntry>())),
            EntryType::LOCAL_APIC_NMI_SRC => Some(MadtEntry::LocalApicNmiSrc(core::ptr::from_ref(self).cast::<LocalApicNmiSrcEntry>())),
            EntryType::LOCAL_APIC_NMI => Some(MadtEntry::LocalApicNmi(core::ptr::from_ref(self).cast::<LocalApicNmiEntry>())),
            EntryType::LOCAL_APIC_ADDR_OVERRIDE => Some(MadtEntry::LocalApicAddrOverride(core::ptr::from_ref(self).cast::<LocalApicAddrOverrideEntry>())),
            EntryType::PROCESSOR_LOCAL_X2APIC => Some(MadtEntry::ProcessorLocalx2Apic(core::ptr::from_ref(self).cast::<ProcessorLocalx2ApicEntry>())),
            _ => None,
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


#[repr(C)]
#[derive(Debug)]
struct LocalApicEntry {
    header: EntryHeader,
    acpi_processor_id: u8,
    apic_id: u8,
    flags: u32,
}

#[repr(C)]
#[derive(Debug)]
struct IoApicEntry {
    header: EntryHeader,
    io_apic_id: u8,
    _reserved: u8,
    io_apic_addr: u32,
    gsi_base: u32,
}

#[repr(C)]
#[derive(Debug)]
struct IoApicIsoEntry {
    header: EntryHeader,
    bus_source: u8,
    irq_source: u8,
    gsi: u32,
    flags: u16,
}

#[repr(C)]
#[derive(Debug)]
struct LocalApicNmiSrcEntry {
    header: EntryHeader,
    nmi_source: u8,
    _reserved: u8,
    flags: u16,
    gsi: u32,
}

#[repr(C)]
#[derive(Debug)]
struct LocalApicNmiEntry {
    header: EntryHeader,
    acpi_processor_id: u8,
    flags: u16,
    lint: u8,
}

#[repr(C)]
#[derive(Debug)]
struct LocalApicAddrOverrideEntry {
    header: EntryHeader,
    _reserved: u16,
    // TODO: Maybe use here a PhysAddr?
    local_apic_phys_addr: u64,
}

#[repr(C)]
#[derive(Debug)]
struct ProcessorLocalx2ApicEntry {
    header: EntryHeader,
    _reserved: u16,
    processor_x2apic_id: u32,
    flags: u32,
    acpi_id: u32,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct Madt {
    header: SdtHeader,
    local_apic_addr: u32,
    flags: u32,
    first_record: EntryHeader,
}

struct Iter {
    current: *const EntryHeader,
    end_addr: usize,
}

impl Madt {
    fn iter(&self) -> Iter {
        Iter {
            current: core::ptr::from_ref(&self.first_record),
            end_addr: core::ptr::from_ref(&self).addr() + self.header.length as usize,
        }
    }
}

impl Iterator for Iter {
    type Item = *const EntryHeader;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: Handle this situation better than just `assert`?
        assert!(self.current.addr() <= self.end_addr);
        if self.current.addr() == self.end_addr {
            return None;
        }

        let ret = self.current;
        self.current = unsafe { 
            let len = (*self.current).length as usize;
            self.current.byte_add(len) 
        };

        Some(ret)
    }
}

impl AcpiTable for Madt {
    const SIGNATURE: &'static [u8; 4] = b"APIC";

    fn validate(&self) -> Result<(), AcpiError> {
        let sum = self.iter().fold(0, |acc, header| {
            let header = unsafe { header.as_ref().unwrap() };
            let entry = header.to_entry().unwrap();
            acc + header.sum_fields() + header.to_entry().unwrap().as_ref().unwrap().sum_fields() + self.header.sum()
        });
        checksums!(sum);

        Ok(())
    }
}

//pub(super) struct Madt {
//    header: SdtHeader,
//
//}
//
//impl AcpiTable for Madt {
//    const SIGNATURE: &'static [u8; 4] = b"APIC";
//}
//
//impl Madt {
//    pub(super) fn 
//}
