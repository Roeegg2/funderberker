//! Parser for the HPET table

use super::{AcpiError, AcpiTable, Gas, SdtHeader};
use crate::{
    arch::x86_64::paging::Entry,
    dev::timer::hpet::{self, InterruptRoutingMode},
    mem::{PhysAddr, vmm::map_page},
};

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
        unsafe {
            let phys_addr = PhysAddr(self.base_addr.addr as usize);
            let virt_addr = map_page(phys_addr, Entry::FLAG_RW);

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
