use core::{mem::transmute, ptr, time::Duration};

use modular_bitfield::prelude::*;
use utils::id_allocator::{Id, IdAllocator};

use crate::{mem::mmio::{MmioArea, Offsetable}, sync::spinlock::{SpinLock, SpinLockDropable, SpinLockGuard}};

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
pub struct Timer {
    area: MmioArea<usize, usize, u64>,
    id: Id,
}

pub struct Hpet {
    area: MmioArea<usize, usize, u64>,
    main_clock_period: u64,
    minimum_tick: u16,
    timer_ids: IdAllocator,
}

// XXX: THIS IS DEFINITELY NOT SAFE TO DO, SINCE IT'S NOT ALWAYS MAPPED ON THE OTHER CORES
unsafe impl Send for Hpet {}
unsafe impl Sync for Hpet {}

// TODO: Move this out of here
const NANO_TO_FEMTOSEC: u128 = 1_000_000;

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

pub static HPET: SpinLock<Hpet> = SpinLock::new(Hpet::DEFAULT());

impl Hpet {
    const MAX_TIMER_AMOUNT: u64 = 32;

    const fn DEFAULT() -> Self {
        Self {
            area: MmioArea::new(ptr::dangling_mut()),
            main_clock_period: 0,
            minimum_tick: 0,
            timer_ids: IdAllocator::uninit(),
        }
    }

    #[inline]
    fn time_to_cycles(&self, time: Duration) -> u64 {
        ((time.as_nanos() * NANO_TO_FEMTOSEC) / (self.main_clock_period as u128)) as u64
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
    /// SAFETY: This function is unsafe because it writes to MMIO registers, which can cause UB
    /// if the parameters passed are not valid.
    #[inline]
    pub unsafe fn init(base: *mut u64, minimum_tick: u16) {
        let mut hpet = HPET.lock();

        *hpet = Hpet::new(base, minimum_tick);
        
        unsafe {
            // Sanity disable the HPET before we do anything
            hpet.set_disable(true);
            // Set and configure the interrupt routing
            hpet.set_interrupt_routing();
            // Reset the main counter value to a known state
            hpet.area.write(WriteableRegs::MAIN_COUNTER_VALUE, 0);
            // Enable the HPET
            hpet.set_disable(false);
        }
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
    ///
    /// SAFETY: This function is unsafe because disabling while the HPET is in use can be UB.
    #[inline]
    pub unsafe fn set_disable(&mut self, state: bool) {
        let mut config: GeneralConfiguration =
            unsafe { transmute(self.area.read(ReadableRegs::GENERAL_CONFIGURATION)) };

        config.set_enable((!state).into());

        unsafe {
            self.area
                .write(WriteableRegs::GENERAL_CONFIGURATION, config.into())
        };
    }
}

impl Timer {
    /// Initialize the timer with the given `time `and `timer_mode`
    unsafe fn init<'a>(&mut self, hpet: SpinLockGuard<'a, Hpet>, time: Duration, timer_mode: TimerMode) -> Result<(), HpetError> {
        let mut config: TimerConfiguration = unsafe { transmute(self.area.read(self.config_reg_offset())) };

        if timer_mode == TimerMode::Periodic && config.periodic_int_capable() == false.into() {
            return Err(HpetError::UnsupportedTimerMode);
        }

        println!("these are the cycles: {:?}", unsafe {
            hpet.area.read(ReadableRegs::MAIN_COUNTER_VALUE)
        });

        let cycles = unsafe {
            hpet.area.read(ReadableRegs::MAIN_COUNTER_VALUE) + hpet.time_to_cycles(time)
        };
        
        drop(hpet);

        unsafe {self.area.write(self.comparator_reg_offset(), cycles)};


        config.set_timer_type(timer_mode as u8);
        config.set_int_type(TriggerMode::EdgeTriggered as u8);
        config.set_int_enable(true.into());

        unsafe {
            self.area.write(self.config_reg_offset(), config.into());
        }

        Ok(())
    }

    /// Allocate a new timer
    #[must_use]
    pub fn new(time: Duration, timer_mode: TimerMode) -> Result<Self, HpetError> {
        let mut hpet = HPET.lock();

        let mut timer = {
            let base = hpet.area.base();
            let id = hpet.timer_ids.allocate().map_err(|_| HpetError::NoFreeTimer)?;

            println!("HPET: Allocated timer ID: {}", id.0);

            Self {
                area: MmioArea::new(base),
                id,
            }
        };

        unsafe {
            timer.init(hpet, time, timer_mode)?;
        }

        Ok(timer)
    }

    /// Disable this specific timer (it just masks off the interrupts, so it's effectively disabled)
    unsafe fn set_disable(&mut self, state: bool) {
        let mut config: TimerConfiguration = unsafe { transmute(self.area.read(self.config_reg_offset())) };

        config.set_int_enable((!state).into());

        unsafe {
            self.area.write(self.config_reg_offset(), config.into());
        }
    }

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

impl Drop for Timer {
    fn drop(&mut self) {
        unsafe {
            self.set_disable(true);
        }
    }
}

impl Offsetable for usize {
    fn offset(self) -> usize {
        self
    }
}

unsafe impl SpinLockDropable for Hpet {}
