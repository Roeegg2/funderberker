//! A PIT driver

use core::time::Duration;

use modular_bitfield::prelude::*;

use crate::{
    arch::x86_64::{apic::ioapic, cpu::outb_8, interrupts},
    sync::spinlock::{SpinLock, SpinLockDropable, SpinLockGuard},
};

use super::{Timer, TimerError};

#[derive(Clone, Copy)]
#[bitfield]
#[repr(u8)]
struct Command {
    channel: B2,
    access_mode: B2,
    operating_mode: B3,
    bcd: B1,
}

/// The PIT has three channels (0, 1, and 2) and a command register.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum ChannelPort {
    Channel0 = 0x40,
    _Channel1 = 0x41,
    _Channel2 = 0x42,
    Command = 0x43,
}

/// The PIT has three channels (0, 1, and 2) and a command register.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum Channel {
    Channel0 = 0b00,
    Channel1 = 0b01,
    Channel2 = 0b10,
    ReadBack = 0b11,
}

/// The available access modes for the PIT. The access mode determines how the channel port is
/// accessed.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum AccessMode {
    Latch = 0b00,
    LowByte = 0b01,
    HighByte = 0b10,
    LowAndHighByte = 0b11,
}

/// The different operating modes of the PIT.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum OperatingMode {
    /// On a `ChannelPort::Command` write the output signal turns 0, until the channel's reload
    /// register is written. When the reload register is written, the `current count` is reloaded on the next falling
    /// edge. Then, each falling edge decrements the `current count` by 1.
    /// During the switch between 1 and 0, the output signal is set to 1. And remains high and so forth
    InterruptOnTerminalCount = 0b000,
    /// Same as `InterruptOnTerminalCount`, but the counting starts on a rising edge instead (with
    /// a few other minor differences, but we don't care for this mode anyway)
    HardwareRetriggerableOneShot = 0b001,
    /// Essentially a frequency divider.
    /// On a `ChannelPort::Command` write the output signal turns 1, until the channel's reload.
    /// When the reload register is written, the `current count` is reloaded on the next falling
    /// edge. Then, each falling edge decrements the `current count` by 1.
    /// During the switch between 2 and 1, the output signal is set to 1. Then on the following
    /// falling edge, the output signal is set to 1 and the `current count` is reloaded once again.
    RateGenerator = 0b010,
    /// The same as `RateGenerator`, but the output is fed into a flip-flop, which produces a
    /// square wave signal. The state of the flip-flop is toggled on each change of the input
    /// state, so it's changed half as often, and so because of that in this mode the `current count` is decremented by 2 each time.
    SquareWaveGenerator = 0b011,
    SoftwareTriggeredStrobe = 0b100,
    HardwareTriggeredStrobe = 0b101,
    _RateGenerator2 = 0b110,
    _SquareWaveGenerator2 = 0b111,
}

// TODO: Overcome the limitation of having this be a non ZST?

pub static PIT: SpinLock<Pit> = SpinLock::new(Pit(0));

#[derive(Debug)]
pub struct Pit(u8);

impl Timer for Pit {
    type TimerMode = OperatingMode;

    fn configure(
        &mut self,
        period: Duration,
        operating_mode: Self::TimerMode,
    ) -> Result<(), TimerError> {
        interrupts::do_inside_interrupts_disabled_window(|| -> Result<(), TimerError> {
            let command = Command::new()
                .with_channel(Channel::Channel0 as u8)
                .with_access_mode(AccessMode::LowAndHighByte as u8)
                .with_operating_mode(operating_mode as u8)
                .with_bcd(false.into());

            let divisor = Pit::time_to_cycles(period, operating_mode)?;

            unsafe {
                self.write(command, divisor);
            }

            Ok(())
        })
    }

    #[inline]
    fn set_disabled(&mut self, status: bool) {
        unsafe {
            ioapic::set_disabled(interrupts::PIT_IRQ, status)
                .expect("Failed to set PIT IRQ disabled");
        }
    }
}

impl Pit {
    fn time_to_cycles(period: Duration, operating_mode: OperatingMode) -> Result<u16, TimerError> {
        const BASE_FREQUENCY: u32 = 1193182; // Hz

        match operating_mode {
            OperatingMode::RateGenerator
            | OperatingMode::SquareWaveGenerator
            | OperatingMode::_RateGenerator2
            | OperatingMode::_SquareWaveGenerator2 => {
                if period.as_micros() == 0 {
                    return Err(TimerError::InvalidTimePeriod);
                }
            }
            _ => (),
        };

        if period.as_micros() == 0 {
            return Err(TimerError::InvalidTimePeriod);
        }

        let mut cycles = (BASE_FREQUENCY / period.as_micros() as u32) as u16;

        if cycles == 0xffff {
            cycles = 0;
        }

        Ok(cycles)
    }

    unsafe fn write(&mut self, command: Command, divisor: u16) {
        unsafe {
            let val: u8 = command.into();
            outb_8(ChannelPort::Command as u16, val);

            outb_8(ChannelPort::Channel0 as u16, (divisor & 0xff) as u8);
            outb_8(ChannelPort::Channel0 as u16, ((divisor >> 8) & 0xff) as u8);
        };
    }
}

impl SpinLockDropable for Pit {
    fn custom_unlock(&mut self) {
        self.set_disabled(true);
    }
}
