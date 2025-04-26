//! Driver for the local APIC timer
//!
//! Each core on the system has it's own timer, so no syncronization is needed

use core::{arch::x86_64::__cpuid_count, time::Duration};

use crate::arch::x86_64::apic::lapic::{LOCAL_APICS, LocalApic};

use super::{Timer, TimerError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerMode {
    OneShot = 0b0,
    Periodic = 0b1,
    Tsc = 0b10,
    _Reserved = 0b11,
}

// TODO: Remove having a APIC field, we should just have a global static

pub struct ApicTimer<'a> {
    /// The frequency of the timer **in Hz**
    frequency: u32,
    /// Caching of whether TSC deadline mode is supported
    tsc_deadline_supported: bool,
    /// The local APIC this timer belongs to
    apic: &'a mut LocalApic,
}

impl<'a> ApicTimer<'a> {
    pub fn new() -> Self {
        // TODO: Check and save info about P-state and C-state transitions

        // TODO: Remove this shit
        let apic = unsafe {
            let this_apic_id = (__cpuid_count(1, 0).ebx >> 24) & 0xff as u32;
            #[allow(static_mut_refs)]
            LOCAL_APICS
                .iter_mut()
                .find(|lapic| lapic.apic_id() == this_apic_id)
                .unwrap()
        };

        let frequency = {
            // Read the divider config
            let divide = apic.get_timer_divide_config();

            // Read the clock crystal frequency
            let res = unsafe { __cpuid_count(0x15, 0x0) };

            // Calculate the APIC timer frequency
            res.ecx / divide as u32
        };

        println!("APIC timer frequency: {} Hz", frequency);

        // Cache the TSC deadline mode support
        let tsc_deadline_supported = {
            let res = unsafe { __cpuid_count(0x1, 0x0) };
            (res.ecx >> 24) & 0b1 == 1
        };

        Self {
            frequency,
            tsc_deadline_supported,
            apic,
        }
    }

    const fn time_to_ticks(&self, time: Duration) -> u32 {
        1000000000
        // (self.frequency as u128 / time.as_micros()) as u32
    }
}

impl<'a> Timer for ApicTimer<'a> {
    type TimerMode = TimerMode;

    fn start(&mut self, time: Duration, timer_mode: Self::TimerMode) -> Result<(), TimerError> {
        if timer_mode == TimerMode::_Reserved
            || (timer_mode == TimerMode::Tsc && !self.tsc_deadline_supported)
        {
            return Err(TimerError::UnsupportedTimerMode);
        }

        let ticks = self.time_to_ticks(time);

        self.apic.configure_timer(ticks, timer_mode);

        self.apic.set_timer_disabled(false);

        Ok(())
    }

    fn set_disabled(&mut self, disable: bool) {
        self.apic.set_timer_disabled(disable);
    }
}
