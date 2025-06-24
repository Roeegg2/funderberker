//! Parser for the HPET table

use super::{AcpiError, AcpiTable, Gas, SdtHeader};
use drivers::timer::hpet::{self, InterruptRoutingMode};
use kernel::{
    arch::x86_64::X86_64,
    mem::paging::{Flags, PageSize, PagingManager},
};
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
            let virt_addr = X86_64::map_pages(
                phys_addr,
                1,
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

        logger::info!("Configured HPET as timer");

        Ok(())
    }
}

impl AcpiTable for Hpet {
    const SIGNATURE: &'static [u8; 4] = b"HPET";
}
