//! RTC driver implementation

use utils::sanity_assert;

use crate::{
    arch::x86_64::{
        apic::ioapic,
        interrupts::{self, RTC_IRQ},
    },
    dev::cmos::{self, CmosIndex, NmiStatus},
    sync::spinlock::{SpinLock, SpinLockable},
};

pub static RTC: SpinLock<Rtc> = SpinLock::new(Rtc {});

unsafe impl Send for Rtc {}
unsafe impl Sync for Rtc {}

#[derive(Debug)]
pub struct Rtc {}

impl Rtc {
    // XXX: Possibly mark this as unsafe, since it could mask out some interrupts because of
    // priority?
    pub fn new_periodic_interrupts(&mut self, nmi_status: NmiStatus) {
        unsafe {
            ioapic::override_irq(RTC_IRQ, RTC_IRQ as u32, 0x0, None)
                .expect("Failed to override IOAPIC IRQ");
        };

        self.set_disabled(false);

        interrupts::do_inside_interrupts_disabled_window(|| {
            let status_b = cmos::read_cmos(CmosIndex::StatusB, nmi_status);

            cmos::write_cmos(CmosIndex::StatusB, status_b | 0x40, nmi_status);
        })
    }

    pub fn change_rate(&mut self, rate: u8, nmi_status: NmiStatus) {
        sanity_assert!(2 < rate && rate < 15);

        interrupts::do_inside_interrupts_disabled_window(|| {
            let status_a = cmos::read_cmos(CmosIndex::StatusA, nmi_status);
            cmos::write_cmos(CmosIndex::StatusA, (status_a & 0xF0) | rate, nmi_status);
        })
    }

    const fn rate_to_frequency(rate: u8) -> u32 {
        sanity_assert!(2 < rate && rate < 15);

        32768 >> (rate - 1)
    }

    #[inline]
    fn set_disabled(&mut self, status: bool) {
        unsafe {
            ioapic::set_disabled(interrupts::RTC_IRQ, status)
                .expect("Failed to set PIT IRQ disabled");
        }
    }
}

impl SpinLockable for Rtc {
    fn custom_unlock(&mut self) {
        println!("RTC: Unlocking RTC");
        // self.set_disabled(true);
    }
}

// don't use weekday register, calculate it from the date instead
//
// set the interrupt on update thing register to read the time and date
// to read time and date, spin until update in progress goes from 1 to 0
//
// you need to handle reading both 24 hour and 12 hour formats, both in BCD and binary since status
// B register cannot always be changed
