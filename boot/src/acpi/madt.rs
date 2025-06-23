//! Parser for the MADT table

use super::{AcpiError, AcpiTable, SdtHeader};
use arch::{
    map_page,
    paging::{Flags, PageSize},
    x86_64::apic::{
        DeliveryMode,
        ioapic::{self, IoApic, init_irq_allocator},
        lapic,
    },
};
use logger::*;
use utils::mem::PhysAddr;

/// A ZST struct for the possible entry types in the MADT
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
    /// A local APIC NMI entry. This is used to configure LINT pins of the local APIC as NMI
    const LOCAL_APIC_NMI: u8 = 4;
    /// A local APIC address override. If defined, use this instead of the address stored in the
    /// MADT header.
    const LOCAL_APIC_ADDR_OVERRIDE: u8 = 5;
    /// Just like the `0` entry, but for x2APIC
    const PROCESSOR_LOCAL_X2APIC: u8 = 9;
}

/// The header that comes before every entry in the MADT
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct EntryHeader {
    /// The type of the entry
    entry_type: u8,
    /// The length of the entry
    length: u8,
}

/// Entry describing a processor and it's local APIC
#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicEntry {
    /// The entry header
    header: EntryHeader,
    /// The ACPI processor ID
    acpi_processor_id: u8,
    /// The local APIC ID
    apic_id: u8,
    /// The flags of the entry
    flags: u32,
}

/// Entry describing an I/O APIC
#[repr(C, packed)]
#[derive(Debug)]
struct IoApicEntry {
    /// The entry header
    header: EntryHeader,
    /// The I/O APIC ID
    io_apic_id: u8,
    /// Reserved
    _reserved: u8,
    /// The physical address of the I/O APIC
    io_apic_addr: u32,
    /// The GSI base of the I/O APIC
    gsi_base: u32,
}

/// Entry describing an I/O APIC interrupt source override
#[repr(C, packed)]
#[derive(Debug)]
struct IoApicIsoEntry {
    /// The entry header
    header: EntryHeader,
    bus_source: u8,
    /// The IRQ source
    irq_source: u8,
    /// The GSI to configure
    gsi: u32,
    /// `PinPolarity` and `TriggerMode` flags
    flags: u16,
}

/// Entry describing an I/O APIC NMI interrupt source override
#[repr(C, packed)]
#[derive(Debug)]
struct IoApicNmiIsoEntry {
    /// The entry header
    header: EntryHeader,
    /// The IRQ source
    nmi_source: u8,
    /// Reserved
    _reserved: u8,
    /// `PinPolarity` and `TriggerMode` flags
    flags: u16,
    /// The GSI to configure
    gsi: u32,
}

/// Entry describing a pin that should be marked as NMI on the local APIC
#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicNmiEntry {
    /// The entry header
    header: EntryHeader,
    /// The ACPI processor ID
    acpi_processor_id: u8,
    /// `PinPolarity` and `TriggerMode` flags
    flags: u16,
    /// The LINT pin to configure
    lint: u8,
}

/// Entry describing a local APIC address override
#[repr(C, packed)]
#[derive(Debug)]
struct LocalApicAddrOverrideEntry {
    /// The entry header
    header: EntryHeader,
    /// Reserved
    _reserved: u16,
    // TODO: Maybe use here a PhysAddr?
    local_apic_phys_addr: u64,
}

/// Entry describing a processor and it's x2APIC
#[repr(C, packed)]
#[derive(Debug)]
struct ProcessorLocalx2ApicEntry {
    /// The entry header
    header: EntryHeader,
    /// Reserved
    _reserved: u16,
    /// the APIC ID of the processor
    x2apic_id: u32,
    /// Flags of the entry
    flags: u32,
    /// The ACPI processor ID
    processor_acpi_id: u32,
}

/// The MADT table
#[repr(C)]
#[derive(Debug)]
pub(super) struct Madt {
    /// The SDT header
    header: SdtHeader,
    /// The default physical base address of the local APICs
    local_apic_addr: u32,
    /// The flags of the MADT
    flags: u32,
}

/// Iterator over the entries in the MADT
struct Iter {
    ptr: *const EntryHeader,
    len: usize,
}

impl Madt {
    /// The offset from the start of the MADT to the entries
    const OFFSET_TO_ENTRIES: usize = 0x2c;

    /// Get an iterator over the entries in the MADT
    fn iter(&self) -> Iter {
        let len = self.header.length as usize - Self::OFFSET_TO_ENTRIES;
        let ptr: *const EntryHeader = unsafe {
            core::ptr::from_ref(self)
                .byte_add(Self::OFFSET_TO_ENTRIES)
                .cast::<EntryHeader>()
        };

        Iter { ptr, len }
    }

    /// Parse the entries of the MADT
    pub(super) fn parse(&self) -> Result<(), AcpiError> {
        self.header.validate_checksum()?;

        // Mask off all of the old PIC interrupts
        unsafe { IoApic::mask_off_pic() };

        for entry in self.iter() {
            let entry_type = unsafe { entry.read().entry_type };
            match entry_type {
                EntryType::LOCAL_APIC => unsafe {
                    let entry = entry.cast::<LocalApicEntry>().as_ref().unwrap();
                    lapic::add(
                        PhysAddr(self.local_apic_addr as usize),
                        entry.acpi_processor_id as u32,
                        entry.apic_id as u32,
                        entry.flags,
                    );
                },
                EntryType::IO_APIC => unsafe {
                    let entry = entry.cast::<IoApicEntry>().as_ref().unwrap();
                    ioapic::add(
                        PhysAddr(entry.io_apic_addr as usize),
                        entry.gsi_base,
                        entry.io_apic_id,
                    );
                },
                EntryType::IO_APIC_ISO => unsafe {
                    let entry = entry.cast::<IoApicIsoEntry>().as_ref().unwrap();
                    ioapic::override_irq(entry.irq_source, entry.gsi, entry.flags, None);
                },
                EntryType::IO_APIC_NMI_ISO => unsafe {
                    let entry = entry.cast::<IoApicNmiIsoEntry>().as_ref().unwrap();
                    ioapic::override_irq(
                        entry.nmi_source,
                        entry.gsi,
                        entry.flags,
                        Some(DeliveryMode::Nmi),
                    );
                },
                EntryType::LOCAL_APIC_NMI => unsafe {
                    let entry = entry.cast::<LocalApicNmiEntry>().as_ref().unwrap();
                    lapic::config_lints(entry.acpi_processor_id as u32, entry.lint, entry.flags);
                },
                EntryType::LOCAL_APIC_ADDR_OVERRIDE => unsafe {
                    // XXX: I think this entry should always come before the local apic and all the
                    // override entries but that might be wrong. But eh womp womp if that's the case I guess
                    let entry = entry.cast::<LocalApicAddrOverrideEntry>().as_ref().unwrap();
                    let ptr: *mut u32 = map_page(
                        PhysAddr(entry.local_apic_phys_addr as usize),
                        Flags::new().set_read_write(true),
                        PageSize::size_4kb(),
                    )
                    .unwrap()
                    .into();
                    lapic::override_base(ptr);
                },
                EntryType::PROCESSOR_LOCAL_X2APIC => unsafe {
                    let entry = entry.cast::<ProcessorLocalx2ApicEntry>().as_ref().unwrap();
                    lapic::add(
                        PhysAddr(self.local_apic_addr as usize),
                        entry.processor_acpi_id,
                        entry.x2apic_id,
                        entry.flags,
                    );
                },
                _ => {
                    log_warn!("APIC: Unknown entry type: {}", entry_type);
                }
            }
        }

        init_irq_allocator();

        Ok(())
    }
}

impl AcpiTable for Madt {
    const SIGNATURE: &'static [u8; 4] = b"APIC";
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
