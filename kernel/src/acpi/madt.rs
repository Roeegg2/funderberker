use super::{AcpiError, AcpiTable, SdtHeader};

use crate::{
    arch::x86_64::apic::{DeliveryMode, ioapic, lapic},
    mem::PhysAddr,
};

#[derive(Debug)]
struct EntryType;

impl EntryType {
    /// A processor and it's LAPIC
    const LOCAL_APIC: u8 = 0;
    /// An I/O APIC
    const IO_APIC: u8 = 1;
    /// An I/O APIC interrupt source override - explains how IRQs are mapped to the global sys
    /// interrupts
    const IO_APIC_ISO: u8 = 2;
    /// An input pin on the I/O APIC that should be marked as NMI
    const IO_APIC_NMI_ISO: u8 = 3;
    const LOCAL_APIC_NMI: u8 = 4;
    /// A local APIC address override. If defined, use this instead of the address stored in the
    /// MADT header.
    const LOCAL_APIC_ADDR_OVERRIDE: u8 = 5;
    /// Just like the `0` entry, but for x2APIC
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
struct IoApicNmiIsoEntry {
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
    x2apic_id: u32,
    flags: u32,
    processor_acpi_id: u32,
}

#[repr(C)]
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
    const OFFSET_TO_ENTRIES: usize = 0x2c;

    fn iter(&self) -> Iter {
        let len = self.header.length as usize - Self::OFFSET_TO_ENTRIES;
        let ptr: *const EntryHeader = unsafe {
            core::ptr::from_ref(self)
                .byte_add(Self::OFFSET_TO_ENTRIES)
                .cast::<EntryHeader>()
        };

        Iter { ptr, len }
    }

    pub(super) fn parse(&self) -> Result<(), AcpiError> {
        unsafe { self.header.validate_checksum()? };

        for entry in self.iter() {
            let entry_type = unsafe { entry.read().entry_type };
            match entry_type {
                EntryType::LOCAL_APIC => {
                    let entry = unsafe { entry.cast::<LocalApicEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        lapic::add(
                            PhysAddr(self.local_apic_addr as usize),
                            entry.acpi_processor_id as u32,
                            entry.apic_id as u32,
                            entry.flags.try_into().unwrap(),
                        )
                    };
                },
                EntryType::IO_APIC => {
                    // TODO: Move this to some global structure
                    let entry = unsafe { entry.cast::<IoApicEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe { ioapic::add(PhysAddr(entry.io_apic_addr as usize), entry.gsi_base) };
                },
                EntryType::IO_APIC_ISO => {
                    let entry = unsafe { entry.cast::<IoApicIsoEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        ioapic::override_irq(
                            entry.irq_source,
                            entry.gsi,
                            entry.flags,
                            DeliveryMode::Fixed,
                        ).expect("Failed to override IOAPIC IRQ");
                    };
                },
                EntryType::IO_APIC_NMI_ISO => {
                    let entry = unsafe { entry.cast::<IoApicNmiIsoEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        ioapic::override_irq(
                            entry.nmi_source,
                            entry.gsi,
                            entry.flags,
                            DeliveryMode::Nmi,
                        ).expect("Failed to override IOAPIC NMI IRQ");
                    };
                },
                EntryType::LOCAL_APIC_NMI => {
                    let entry = unsafe { entry.cast::<LocalApicNmiEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        lapic::set_as_nmi(
                            entry.acpi_processor_id as u32,
                            entry.lint,
                            entry.flags.try_into().unwrap(),
                        );
                    };
                },
                EntryType::LOCAL_APIC_ADDR_OVERRIDE => {
                    // XXX: I think this entry should always come before the local apic and all the
                    // override entries but that might be wrong. Fuck it if that's the case I guess
                    let entry =
                        unsafe { entry.cast::<LocalApicAddrOverrideEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        lapic::set_base(
                            PhysAddr(entry.local_apic_phys_addr as usize)
                                .add_hhdm_offset()
                                .into(),
                        )
                    };
                },
                EntryType::PROCESSOR_LOCAL_X2APIC => {
                    let entry =
                        unsafe { entry.cast::<ProcessorLocalx2ApicEntry>().as_ref().unwrap() };
                    println!("{:?}", entry);
                    unsafe {
                        lapic::add(
                            PhysAddr(self.local_apic_addr as usize),
                            entry.processor_acpi_id,
                            entry.x2apic_id,
                            entry.flags.try_into().unwrap(),
                        )
                    };
                },
                _ => {
                    log_warn!("MADT: Unknown entry type: {}", entry_type)
                }
            }
        }

        Ok(())
    }
}

struct Iter {
    ptr: *const EntryHeader,
    len: usize,
}

impl Iterator for Iter {
    type Item = *const EntryHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ptr: *const EntryHeader = self.ptr;

        let len = unsafe { (*ptr).length as usize };
        self.len -= len;
        self.ptr = unsafe { self.ptr.byte_add(len) };

        Some(ptr)
    }
}
