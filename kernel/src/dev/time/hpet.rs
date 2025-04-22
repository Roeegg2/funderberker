use core::mem::transmute;

use alloc::vec;
use alloc::vec::Vec;
use modular_bitfield::prelude::*;
use utils::const_max;

use crate::mem::mmio::{MmioArea, Offsetable};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HpetError {
    NoFreeTimer,
    NoSuchTimer,
    UnsupportedTimerMode,
    InvalidTimePeriod,
    UnusedTimer,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerMode {
    EdgeTriggered = 0b0,
    LevelTriggered = 0b1,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerMode {
    OneShot = 0b0,
    Periodic = 0b1,
}

struct ReadableRegs;

struct WriteableRegs;

#[bitfield]
#[derive(Clone, Copy)]
#[repr(u64)]
struct GeneralCapabilities {
    rev_id: B8,
    num_tim_cap: B5,
    count_size_cap: B1,
    _reserved: B1,
    leg_route_cap: B1,
    vendor_id: u16,
    counter_clock_period: u32,
}

#[bitfield]
#[derive(Clone, Copy)]
#[repr(u64)]
struct GeneralConfiguration {
    enable: B1,
    legacy_route: B1,
    _reserved1: B62,
}

type GeneralInterruptStatusValue = u64;

type MainCounterValue = u64;

type TimerComparator = u64;

#[bitfield]
#[derive(Clone, Copy)]
#[repr(u64)]
struct TimerConfiguration {
    _reserved0: B1,
    int_type: B1,
    int_enable: B1,
    timer_type: B1,
    periodic_int_capable: B1,
    size_capable: B1,
    value_set: B1,
    _reserved1: B1,
    timer_32bit_mode: B1,
    int_route: B5,
    fsb_int_enable: B1,
    fsb_int_delivery: B1,
    _reserved2: B48,
}

#[derive(Clone, Copy)]
#[repr(packed, C)]
struct TimerFsbInterruptRoute {
    fsb_int_val: u32,
    dsb_int_addr: u32,
}

#[derive(Clone, Copy)]
struct Timer {
    cycles_per_period: u64,
    trigger_mode: TriggerMode,
    mode: TimerMode,
}

pub struct Hpet {
    area: MmioArea<usize, usize, u64>,
    main_clock_period: u64,
    minimum_tick: u16,
    timers: Vec<Option<Timer>>,
}

// TODO: Move this out of here
const FEMTOSEC: u64 = 10_u64.pow(15);

impl ReadableRegs {
    const GENERAL_CAPABILITIES: usize = 0x0;
    const GENERAL_CONFIGURATION: usize = 0x10;
    const GENERAL_INTERRUPT_STATUS: usize = 0x20;
    const MAIN_COUNTER_VALUE: usize = 0xf0;
}

impl WriteableRegs {
    const GENERAL_CONFIGURATION: usize = 0x10;
    const GENERAL_INTERRUPT_STATUS: usize = 0x20;
    const MAIN_COUNTER_VALUE: usize = 0xf0;
}

impl Hpet {
    // TODO: Possibly support other interrupt routing methods as well?
    #[inline]
    unsafe fn set_interrupt_routing(&mut self) {
        // Make sure it's supported
        let capabilities: GeneralCapabilities =
            unsafe { transmute(self.area.read(ReadableRegs::GENERAL_CAPABILITIES)) };
        assert!(
            capabilities.leg_route_cap() == true.into(),
            "HPET: Legacy routing not supported"
        );

        // Enable legacy routing
        let mut config: GeneralConfiguration =
            unsafe { transmute(self.area.read(ReadableRegs::GENERAL_CONFIGURATION)) };
        config.set_legacy_route(true.into());
        unsafe {
            self.area
                .write(WriteableRegs::GENERAL_CONFIGURATION, config.into())
        };
    }

    /// Initialize the HPET
    #[inline]
    unsafe fn init(&mut self) {
        unsafe {
            // Sanity disable the HPET before we do anything
            self.set_state(false);
            // Set and configure the interrupt routing
            self.set_interrupt_routing();
            // Reset the main counter value to a known state
            self.area.write(WriteableRegs::MAIN_COUNTER_VALUE, 0);
            // Enable the HPET
            self.set_state(true);
        }
    }

    /// Create the new HPET instance
    pub unsafe fn new(base: *mut u64, minimum_tick: u16) -> Self {
        let mut hpet = Self {
            area: MmioArea::new(base),
            main_clock_period: 0,
            minimum_tick,
            timers: Vec::new(),
        };

        // Get the main clock's period
        hpet.main_clock_period = {
            let capabilities: GeneralCapabilities =
                unsafe { transmute(hpet.area.read(ReadableRegs::GENERAL_CAPABILITIES)) };
            utils::sanity_assert!(capabilities.counter_clock_period() != 0);
            utils::sanity_assert!(capabilities.counter_clock_period() < 0x5F5E100);

            capabilities.counter_clock_period().into()
        };

        // Get the number of timers
        let max_timer_amount = {
            let capabilities: GeneralCapabilities =
                unsafe { transmute(hpet.area.read(ReadableRegs::GENERAL_CAPABILITIES)) };
            let max_timer_index: u64 = capabilities.num_tim_cap().into();
            utils::sanity_assert!(max_timer_index < 32);
            max_timer_index as usize + 1
        };

        hpet.timers = vec![None; max_timer_amount];

        // Initialize the main counter & other stuff
        unsafe { hpet.init() };

        hpet
    }

    /// Enable/disable the HPET (halt the main counter, effectively disabling all the timers)
    #[inline]
    unsafe fn set_state(&mut self, state: bool) {
        let mut config: GeneralConfiguration =
            unsafe { transmute(self.area.read(ReadableRegs::GENERAL_CONFIGURATION)) };

        config.set_enable(state.into());

        unsafe {
            self.area
                .write(WriteableRegs::GENERAL_CONFIGURATION, config.into())
        };
    }

    /// Get the status of the interrupt line of the timer with index `timer_index`
    pub fn get_timer_interrupt_status(&mut self, timer_index: usize) -> Result<bool, HpetError> {
        let timer = self
            .timers
            .get(timer_index)
            .ok_or(HpetError::NoSuchTimer)?
            .as_ref()
            .ok_or(HpetError::UnusedTimer)?;

        let general_interrupt_status =
            unsafe { self.area.read(ReadableRegs::GENERAL_INTERRUPT_STATUS) };

        let mask = 1 << timer_index;
        let status = (general_interrupt_status & mask) != 0;
        if status && timer.trigger_mode == TriggerMode::LevelTriggered {
            unsafe {
                self.area
                    .write(WriteableRegs::GENERAL_INTERRUPT_STATUS, mask)
            };
        }

        Ok(status)
    }

    /// Initialize and enable the timer with index `timer_index`
    #[inline]
    unsafe fn init_timer(
        &mut self,
        timer_index: usize,
        mut timer_config: TimerConfiguration,
        cycles_per_period: u64,
        mode: TimerMode,
        trigger_mode: TriggerMode,
    ) {
        unsafe {
            let cycles = self.area.read(ReadableRegs::MAIN_COUNTER_VALUE) + cycles_per_period;

            self.area
                .write(Timer::index_to_comparator_reg(timer_index), cycles);
        }

        timer_config.set_timer_type((mode as u8).into());
        timer_config.set_int_type((trigger_mode as u8).into());
        timer_config.set_int_enable(true.into());

        let time: u64 = timer_config.into();
        unsafe {
            self.area
                .write(Timer::index_to_config_reg(timer_index), timer_config.into())
        };
    }

    /// Allocate a timer with the given parameters and initialize it
    pub fn alloc_timer(
        &mut self,
        time: u64,
        mode: TimerMode,
        trigger_mode: TriggerMode,
    ) -> Result<usize, HpetError> {
        if time as u64 > FEMTOSEC {
            return Err(HpetError::InvalidTimePeriod);
        }

        let index = self
            .timers
            .iter()
            .position(|x| x.is_none())
            .ok_or(HpetError::NoFreeTimer)?;

        let timer_config: TimerConfiguration =
            unsafe { transmute(self.area.read(Timer::index_to_config_reg(index))) };
        if mode == TimerMode::Periodic && timer_config.periodic_int_capable() == false.into() {
            return Err(HpetError::UnsupportedTimerMode);
        }

        let cycles_per_period = self.femtosec_to_cycles(time);
        self.timers[index] = Some(Timer::new(cycles_per_period, trigger_mode, mode));

        unsafe { self.init_timer(index, timer_config, cycles_per_period, mode, trigger_mode) };

        Ok(index)
    }

    /// Disable and free the timer with the given index
    pub fn free_timer(&mut self, index: usize) -> Result<(), HpetError> {
        self.timers
            .get_mut(index)
            .ok_or(HpetError::NoSuchTimer)?
            .as_mut()
            .ok_or(HpetError::UnusedTimer)?;

        let mut timer_config: TimerConfiguration =
            unsafe { transmute(self.area.read(Timer::index_to_config_reg(index))) };

        if timer_config.int_enable() == false.into() {
            return Err(HpetError::UnusedTimer);
        }

        timer_config.set_int_enable(false.into());
        unsafe {
            self.area
                .write(Timer::index_to_config_reg(index), timer_config.into())
        };

        self.timers[index] = None;

        Ok(())
    }

    #[inline]
    const fn femtosec_to_cycles(&self, time: u64) -> u64 {
        const_max!(
            u64::div_ceil(time, self.main_clock_period),
            self.minimum_tick as u64
        )
    }
}

impl Timer {
    /// Create a new timer with the given parameters
    #[inline]
    const fn new(cycles_per_period: u64, trigger_mode: TriggerMode, mode: TimerMode) -> Self {
        Self {
            cycles_per_period,
            trigger_mode,
            mode,
        }
    }

    /// Get the timer's `TimerConfiguration` register address
    #[inline]
    const fn index_to_config_reg(index: usize) -> usize {
        0x100 + (0x20 * index)
    }

    /// Get the timer's `TimerComparator` register address
    #[inline]
    const fn index_to_comparator_reg(index: usize) -> usize {
        0x108 + (0x20 * index)
    }

    /// Get the timer's `TimerFsbInterruptRoute` register address
    #[inline]
    const fn index_to_fsb_interrupt_route_reg(index: usize) -> usize {
        0x110 + (0x20 * index)
    }
}

impl Offsetable for usize {
    fn offset(self) -> usize {
        self
    }
}

// TODO:
// 1. add different interrupt routing methods
// 2. add support for different timer types (periodic, one-shot)
// 3.
//
// IMPLEMENT DROP!
