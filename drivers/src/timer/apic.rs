//! Local APIC timer driver implementation
//!
//! Each core on the system has it's own timer, so no syncronization is needed

use logger::*;
use core::{arch::x86_64::__cpuid_count, hint, time::Duration};
use arch::x86_64::{apic::lapic::{LocalApic, TimerDivisor, TimerMode}, event::__isr_stub_generic_irq_isr};
use super::{
    Timer, TimerError,
    hpet::{self, AdditionalConfig, DeliveryMode, HPET, HpetTimer, TriggerMode},
};

// TODO: Remove having a APIC field, we should just have a global static

/// The local APIC timer instance
pub struct ApicTimer {
    /// The **base** frequency of the timer **in MHz**.
    /// To get the actual frequency, divide this by the divisor (which doesn't really matter
    /// since we set it to 1 when creating the timer instance)
    base_frequency: u32,
    /// Caching of whether TSC deadline mode is supported
    tsc_deadline_supported: bool,
    /// The local APIC this timer belongs to
    apic_id: u32,
}

impl ApicTimer {
    /// Create a new APIC timer instance
    pub fn new() -> Self {
        // TODO: Check and save info about P-state and C-state transitions

        let apic_id = LocalApic::get_this_apic_id();

        let base_frequency = Self::find_base_frequency(apic_id);

        log_info!("APIC timer frequency: {} Mhz", base_frequency);

        // Cache the TSC deadline mode support
        let tsc_deadline_supported = {
            let res = unsafe { __cpuid_count(0x1, 0x0) };
            (res.ecx >> 24) & 0b1 == 1
        };

        Self {
            base_frequency,
            tsc_deadline_supported,
            apic_id,
        }
    }

    /// Finds the raw frequency (ie frequency before dividing by the dividor) of the timer
    fn find_base_frequency(apic_id: u32) -> u32 {
        let res = unsafe { __cpuid_count(0x15, 0x0) };

        // If these 2 aren't 0, then we can use CPUID result to read the frequency
        if res.ecx != 0 && res.ebx != 0 {
            res.ecx
        } else {
            // If we can't read it from the CPUID, we need to calculate it using HPET:
            let mut hpet_timer = HpetTimer::new().unwrap();

            // Translate the 100ms to ticks
            let ticks = {
                let hpet = HPET.lock();
                hpet.time_to_cycles(Duration::from_millis(1000))
            };

            let apic = LocalApic::get_apic(apic_id);

            // Set the divisor to 1, we want the timer to tick as fast as possible
            apic.set_timer_divider_config(TimerDivisor::Div1);
            // Configure the 2 timers to tick for a period longer than 100ms
            apic.config_timer(u32::MAX, TimerMode::OneShot);

            // We configure the HPET timer.
            //
            // We make sure we don't recieve the interrupts since we want to just poll, and so we
            // use `EdgeTriggered` as well
            //
            // We configure it to run for 5000 just to make sure it doesn't interfere with out
            // measurement
            hpet_timer
                .configure(
                    Duration::from_secs(5000),
                    hpet::TimerMode::OneShot,
                    AdditionalConfig {
                        receive_interrupts: false,
                        delivery_mode: DeliveryMode::Interrupt(
                            __isr_stub_generic_irq_isr,
                            TriggerMode::EdgeTriggered,
                        ),
                    },
                )
                .unwrap();

            // Enable both timers
            apic.set_timer_disabled(false);

            // Poll until we reached 100ms mark, then disable the timers
            loop {
                hint::spin_loop();
                let hpet_timer_count = hpet_timer.read_main_counter();
                if hpet_timer_count >= ticks {
                    apic.set_timer_disabled(true);
                    hpet_timer.set_disabled(true);
                    break;
                }
            }

            // Find the delta (intial tick count - current tick count)
            let ticks_delta = u32::MAX - apic.read_current_timer_count();

            // NOTE: We technically need to mult `ticks_delta` by the TimerDivisor, but we set it
            // to 1 so we don't need to worry about it
            ticks_delta / 1_000_000 // Convert to MHz
        }
    }

    /// Convert a `Duration` into APIC timer clock ticks
    const fn time_to_ticks(&self, time: Duration) -> u32 {
        (self.base_frequency * 1_000_000) / time.as_micros() as u32
    }
}

impl Timer for ApicTimer {
    type TimerMode = TimerMode;
    type AdditionalConfig = ();

    fn configure(
        &mut self,
        time: Duration,
        timer_mode: Self::TimerMode,
        _additional_config: Self::AdditionalConfig,
    ) -> Result<u64, TimerError> {
        // If the mode is reserved, or TSC but TSC isn't enabled then error out
        if timer_mode == TimerMode::_Reserved
            || (timer_mode == TimerMode::Tsc && !self.tsc_deadline_supported)
        {
            return Err(TimerError::UnsupportedTimerMode);
        }

        // Config and initialize the timer
        let ticks = self.time_to_ticks(time);

        let apic = LocalApic::get_apic(self.apic_id);
        apic.config_timer(ticks, timer_mode);
        apic.set_timer_disabled(false);

        Ok(ticks as u64)
    }

    fn set_disabled(&mut self, disable: bool) {
        let apic = LocalApic::get_apic(self.apic_id);

        apic.set_timer_disabled(disable);
    }
}
