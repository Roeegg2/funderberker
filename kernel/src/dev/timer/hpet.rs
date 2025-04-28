//! An HPET driver

use core::{mem::transmute, ptr, time::Duration};

use modular_bitfield::prelude::*;
use utils::id_allocator::{Id, IdAllocator};

use crate::{
    mem::mmio::{MmioArea, Offsetable},
    sync::spinlock::{SpinLock, SpinLockDropable},
};

use super::{Timer, TimerError};

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
    _reserved2: B16,
    int_route_cap: B32,
}

#[derive(Clone, Copy)]
#[repr(packed, C)]
struct TimerFsbInterruptRoute {
    fsb_int_val: u32,
    dsb_int_addr: u32,
}

// NOTE: I could make this a ZST, but I don't think it's worth the trouble
/// A HPET specific timer
pub struct HpetTimer {
    area: MmioArea<usize, usize, u64>,
    id: Id,
}

pub struct Hpet {
    area: MmioArea<usize, usize, u64>,
    main_clock_period: u64,
    minimum_tick: u16,
    timer_ids: IdAllocator,
}

// TODO: Move this out of here
const NANO_TO_FEMTOSEC: u128 = 1_000_000;

pub static HPET: SpinLock<Hpet> = SpinLock::new(Hpet {
    area: MmioArea::new(ptr::dangling_mut()),
    main_clock_period: 0,
    minimum_tick: 0,
    timer_ids: IdAllocator::uninit(),
});

impl Hpet {
    /// The maximum amount of timers supported by the HPET
    ///
    /// NOTE: This is not a guarantee, but a limit. The hardware might have less (usually it has 3)
    const MAX_TIMER_AMOUNT: u64 = 32;

    /// Converts the time to cycles.
    ///
    /// IMPORTANT NODE: If the time is not a multiple of the main clock period, it will be rounded
    /// up to the next multiple of the main clock period.
    #[inline]
    pub const fn time_to_cycles(&self, time: Duration) -> u64 {
        let diff = (time.as_nanos() * NANO_TO_FEMTOSEC) % (self.main_clock_period as u128);

        (((time.as_nanos() * NANO_TO_FEMTOSEC) + diff) / (self.main_clock_period as u128)) as u64
    }

    // TODO: Possibly support other interrupt routing methods as well?
    // TODO: Move away from transmute?
    /// Set the HPETs interrupt routing mode
    ///
    /// SAFETY: This function is unsafe because calling it not during initialization can cause UB.
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
    ///
    /// NOTE: This enabled the HPET, but in order for the HPET to send interrupts we need to unmask
    /// the interrupts in the IOAPIC as well.
    /// This will be done only later on the general secondary timer initialization.
    ///
    /// SAFETY: This function is unsafe because it writes to MMIO registers, which can cause UB
    /// if the parameters passed are not valid.
    #[inline]
    pub unsafe fn init(base: *mut u64, minimum_tick: u16) {
        let mut hpet = HPET.lock();

        *hpet = Hpet::new(base, minimum_tick);

        // Sanity disable the HPET before we do anything
        hpet.set_disabled(true);
        unsafe {
            // Set and configure the interrupt routing
            hpet.set_interrupt_routing();
            // Reset the main counter value to a known state
            hpet.area.write(WriteableRegs::MAIN_COUNTER_VALUE, 0);
        }

        // Enable the HPET
        hpet.set_disabled(false);
    }

    /// Create the new HPET instance
    fn new(base: *mut u64, minimum_tick: u16) -> Self {
        let mut hpet = Self {
            area: MmioArea::new(base),
            main_clock_period: 0,
            minimum_tick,
            timer_ids: IdAllocator::uninit(),
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
            utils::sanity_assert!(max_timer_index < Self::MAX_TIMER_AMOUNT);
            max_timer_index as usize + 1
        };

        hpet.timer_ids = IdAllocator::new(Id(0)..Id(max_timer_amount));

        hpet
    }

    /// Enable/disable the HPET (halt the main counter, effectively disabling all the timers)
    #[inline]
    pub fn set_disabled(&mut self, state: bool) {
        let mut config: GeneralConfiguration =
            unsafe { transmute(self.area.read(ReadableRegs::GENERAL_CONFIGURATION)) };

        config.set_enable((!state).into());

        unsafe {
            self.area
                .write(WriteableRegs::GENERAL_CONFIGURATION, config.into())
        };
    }
}

impl HpetTimer {
    /// Initialize the timer with the given `time `and `timer_mode`
    #[must_use]
    pub fn new() -> Result<Self, TimerError> {
        let mut hpet = HPET.lock();

        let base = hpet.area.base();
        let id = hpet
            .timer_ids
            .allocate()
            .map_err(|_| TimerError::NoTimerAvailable)?;

        // TODO: Remove this limitation someday by allocating IRQ lines on IOAPIC so we could
        // allocate other timers than just 0
        assert!(id.0 == 0, "HPET: Only timer 0 is supported currently");

        Ok(Self {
            area: MmioArea::new(base),
            id,
        })
    }

    /// Check if the timer has fired
    ///
    /// **NOTE:** This is only valid for timers where the interrupts are level triggered.
    /// For edge triggered interrupts, the status bit will always be set to `0` and so `false` will be
    /// returned.
    #[inline]
    pub fn get_status(&self) -> bool {
        unsafe {
            let read = self.area.read(ReadableRegs::GENERAL_INTERRUPT_STATUS) & (1 << self.id.0);
            // If the timer has fired, we write 1 to clear the status bit
            if read != 0 {
                self.area
                    .write(WriteableRegs::GENERAL_INTERRUPT_STATUS, read);

                return true;
            }

            false
        }
    }

    /// Read the main counter value
    #[inline]
    pub fn read_main_counter(&self) -> u64 {
        unsafe { self.area.read(ReadableRegs::MAIN_COUNTER_VALUE) }
    }

    // /// Resets the main counter value to 0
    // ///
    // /// SAFETY: This function is unsafe because it can mess up the other timers waiting time, so
    // /// make sure no other timers are being used before using this.
    // #[inline]
    // pub unsafe fn reset_main_counter(&self) {
    //     unsafe {
    //         self.area.write(WriteableRegs::MAIN_COUNTER_VALUE, 0);
    //     }
    // }

    /// Get the timer's `TimerConfiguration` register address
    #[inline]
    const fn config_reg_offset(&self) -> usize {
        0x100 + (0x20 * self.id.0)
    }

    /// Get the timer's `TimerComparator` register address
    #[inline]
    const fn comparator_reg_offset(&self) -> usize {
        0x108 + (0x20 * self.id.0)
    }

    /// Get the timer's `TimerFsbInterruptRoute` register address
    #[inline]
    const fn fsb_interrupt_route_reg_offset(&self) -> usize {
        0x110 + (0x20 * self.id.0)
    }

    /// Get the timer's ID
    #[inline]
    pub const fn id(&self) -> Id {
        self.id
    }
}

impl Timer for HpetTimer {
    type TimerMode = TimerMode;

    fn configure(&mut self, time: Duration, timer_mode: TimerMode) -> Result<u64, TimerError> {
        let hpet = HPET.lock();

        let mut config: TimerConfiguration =
            unsafe { transmute(self.area.read(self.config_reg_offset())) };

        if timer_mode == TimerMode::Periodic && config.periodic_int_capable() == false.into() {
            return Err(TimerError::UnsupportedTimerMode);
        }

        let cycles =
            unsafe { hpet.area.read(ReadableRegs::MAIN_COUNTER_VALUE) + hpet.time_to_cycles(time) };

        drop(hpet);

        unsafe { self.area.write(self.comparator_reg_offset(), cycles) };

        config.set_timer_type(timer_mode as u8);
        config.set_int_type(TriggerMode::LevelTriggered as u8);
        config.set_int_enable(true.into());

        unsafe {
            self.area.write(self.config_reg_offset(), config.into());
        }

        Ok(cycles)
    }

    /// Disable this specific timer (it just masks off the interrupts, so it's effectively disabled)
    fn set_disabled(&mut self, state: bool) {
        let mut config: TimerConfiguration =
            unsafe { transmute(self.area.read(self.config_reg_offset())) };

        config.set_int_enable((!state).into());

        unsafe {
            self.area.write(self.config_reg_offset(), config.into());
        }
    }
}

impl SpinLockDropable for HpetTimer {
    fn custom_unlock(&mut self) {
        self.set_disabled(true);
    }
}

impl Offsetable for usize {
    fn offset(self) -> usize {
        self
    }
}

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

unsafe impl Send for Hpet {}
unsafe impl Sync for Hpet {}

unsafe impl Send for HpetTimer {}
unsafe impl Sync for HpetTimer {}

impl SpinLockDropable for Hpet {}
