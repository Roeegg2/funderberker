use crate::{
    arch::x86_64::paging::{Entry, PageSize, PageTable},
    dev::time::hpet::{self, TimerMode, TriggerMode},
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
            let mut hpet = hpet::Hpet::new(virt_addr.into(), self.minimum_tick);

            let timer = hpet
                .alloc_timer(41666667 * 2, TimerMode::OneShot, TriggerMode::EdgeTriggered)
                .unwrap();
        }

        println!("HPET");

        Ok(())
    }
}

impl AcpiTable for Hpet {
    const SIGNATURE: &'static [u8; 4] = b"HPET";
}
