//! Parser for the HPET table

use super::{AcpiError, AcpiTable, Gas, SdtHeader};
use arch::{
    map_page,
    paging::{Flags, PageSize},
};
use drivers::timer::hpet::{self, InterruptRoutingMode};
use logger::*;
use utils::mem::PhysAddr;

/// The HPET table structure
#[repr(C)]
#[derive(Debug)]
pub(super) struct Hpet {
    header: SdtHeader,
    event_timer_block_id: u32,
    base_addr: Gas,
    minimum_tick: u16,
    page_protection_n_oem_attr: u8,
}

impl Hpet {
    pub fn setup_hpet(&self) -> Result<(), AcpiError> {
        self.header.validate_checksum()?;

        // SAFETY: This should be OK since we're mapping a physical address that is marked as
        // reserved, so the kernel shouldn't be tracking it
        let phys_addr = PhysAddr(self.base_addr.addr as usize);
        unsafe {
            let virt_addr = map_page(
                phys_addr,
                Flags::new().set_read_write(true),
                PageSize::size_4kb(),
            )
            .unwrap();

            hpet::Hpet::init(
                virt_addr.into(),
                self.minimum_tick,
                InterruptRoutingMode::Legacy,
            );
        }

        log_info!("Configured HPET as timer");

        Ok(())
    }
}

impl AcpiTable for Hpet {
    const SIGNATURE: &'static [u8; 4] = b"HPET";
}
