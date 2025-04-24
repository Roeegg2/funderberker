use crate::{
    arch::x86_64::{cpu::{cli, sti}, paging::{Entry, PageSize, PageTable}},
    dev::timer::hpet::{self, TimerMode, TriggerMode},
    mem::PhysAddr,
};

use super::{AcpiError, AcpiTable, SdtHeader};

#[repr(C, packed)]
#[derive(Debug)]
struct Addr {
    addr_space_id: u8,
    register_bit_width: u8,
    register_bit_offset: u8,
    _reserved: u8,
    addr: u64,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct Hpet {
    header: SdtHeader,
    event_timer_block_id: u32,
    base_addr: Addr,
    minimum_tick: u16,
    page_protection_n_oem_attr: u8,
}

impl Hpet {
    pub fn parse(&self) -> Result<(), AcpiError> {
        unsafe { self.header.validate_checksum()? };

        let phys_addr = PhysAddr(self.base_addr.addr as usize);
        let virt_addr = phys_addr.add_hhdm_offset();
        // XXX: This might fuck things up very badly, since we're mapping without letting the
        // allocator know, but AFAIK the address the local APIC is mapped to never appears on the
        // memory map
        PageTable::map_page_specific(
            virt_addr,
            phys_addr,
            Entry::FLAG_P | Entry::FLAG_RW | Entry::FLAG_PCD,
            PageSize::Size4KB,
        )
        .unwrap();

        unsafe {
            hpet::Hpet::init(virt_addr.into(), self.minimum_tick);
        }

        println!("HPET");

        Ok(())
    }
}

impl AcpiTable for Hpet {
    const SIGNATURE: &'static [u8; 4] = b"HPET";
}
