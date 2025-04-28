//! Driver for the local APIC timer
//!
//! Each core on the system has it's own timer, so no syncronization is needed

use core::{arch::x86_64::__cpuid_count, hint, mem::transmute, time::Duration, u32};

use crate::arch::x86_64::apic::lapic::{LOCAL_APICS, LocalApic};

use super::{
    Timer, TimerError,
    hpet::{self, HPET, HpetTimer},
};

/// The possible divider values for the APIC timer
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum TimerDivisor {
    /// Divide by 2
    Div2 = 0b000,
    /// Divide by 4
    Div4 = 0b001,
    /// Divide by 8
    Div8 = 0b010,
    /// Divide by 16
    Div16 = 0b011,
    /// Divide by 32
    Div32 = 0b100,
    /// Divide by 64
    Div64 = 0b101,
    /// Divide by 128
    Div128 = 0b110,
    /// Divide by 1
    Div1 = 0b111,
}

/// The local APICs possible timer modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerMode {
    /// The timer will tick the specified amount and then stop
    OneShot = 0b0,
    /// The timer will tick the specified amount regularly
    Periodic = 0b1,
    /// The timer will tick until the specified amount matches the current TSC value
    Tsc = 0b10,
    /// Reserved
    _Reserved = 0b11,
}

// TODO: Remove having a APIC field, we should just have a global static

/// The local APIC timer instance
pub struct ApicTimer<'a> {
    /// The **base** frequency of the timer **in MHz**.
    /// To get the actual frequency, divide this by the divisor (which doesn't really matter
    /// since we set it to 1 when creating the timer instance)
    base_frequency: u32,
    /// Caching of whether TSC deadline mode is supported
    tsc_deadline_supported: bool,
    /// The local APIC this timer belongs to
    apic: &'a mut LocalApic,
}

impl<'a> ApicTimer<'a> {
    /// Create a new APIC timer instance
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

        let base_frequency = Self::find_base_frequency(apic);

        log_info!("APIC timer frequency: {} Mhz", base_frequency);

        // Cache the TSC deadline mode support
        let tsc_deadline_supported = {
            let res = unsafe { __cpuid_count(0x1, 0x0) };
            (res.ecx >> 24) & 0b1 == 1
        };

        Self {
            base_frequency,
            tsc_deadline_supported,
            apic,
        }
    }

    /// Finds the raw frequency (ie frequency before dividing by the dividor) of the timer
    fn find_base_frequency(apic: &mut LocalApic) -> u32 {
        let res = unsafe { __cpuid_count(0x15, 0x0) };

        // If these 2 aren't 0, then we can use CPUID to read the frequency
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

            // Set the divisor to 1, we want the timer to tick as fast as possible
            apic.set_timer_divider_config(TimerDivisor::Div1);
            // Configure the 2 timers to tick for a period longer than 100ms
            apic.configure_timer(u32::MAX, TimerMode::OneShot);
            hpet_timer
                .configure(Duration::from_secs(5000), hpet::TimerMode::OneShot)
                .unwrap();

            // Enable both timers
            hpet_timer.set_disabled(false);
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

    const fn time_to_ticks(&self, time: Duration) -> u32 {
        (self.base_frequency * 1_000_000) / time.as_micros() as u32
    }
}

impl TimerDivisor {
    pub const fn from_bits(self) -> u8 {
        2_u8.pow(((self as u32) + 1) & 0b111)
    }

    pub const fn into_bits(val: u8) -> Option<Self> {
        // if !val.is_power_of_two() || val > 128 {
        //     return None;
        // }
        // this isa  test write

        match val {
            1 => Some(TimerDivisor::Div1),
            2 => Some(TimerDivisor::Div2),
            4 => Some(TimerDivisor::Div4),
            8 => Some(TimerDivisor::Div8),
            16 => Some(TimerDivisor::Div16),
            32 => Some(TimerDivisor::Div32),
            64 => Some(TimerDivisor::Div64),
            128 => Some(TimerDivisor::Div128),
            _ => None,
        }

        // let val = (val.ilog2().wrapping_sub(1) & 0b111) as u8;
        // Some(unsafe { transmute(val) })
    }
}

impl<'a> Timer for ApicTimer<'a> {
    type TimerMode = TimerMode;

    fn configure(
        &mut self,
        time: Duration,
        timer_mode: Self::TimerMode,
    ) -> Result<u64, TimerError> {
        // If the mode is reserved, or TSC but TSC isn't enabled then error out
        if timer_mode == TimerMode::_Reserved
            || (timer_mode == TimerMode::Tsc && !self.tsc_deadline_supported)
        {
            return Err(TimerError::UnsupportedTimerMode);
        }

        // Config and initialize the timer
        let ticks = self.time_to_ticks(time);
        self.apic.configure_timer(ticks, timer_mode);
        self.apic.set_timer_disabled(false);

        Ok(ticks as u64)
    }

    fn set_disabled(&mut self, disable: bool) {
        self.apic.set_timer_disabled(disable);
    }
}
