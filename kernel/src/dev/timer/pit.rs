use core::time::Duration;

use modular_bitfield::prelude::*;

/// Possible PIT errors
#[derive(Debug, Clone, Copy)]
pub enum PitError {
    InvalidTimePeriod,
    InvalidDivisor,
}

use crate::{
    arch::x86_64::{
        apic::ioapic,
        cpu::outb_8,
        interrupts,
    },
    sync::spinlock::{SpinLock, SpinLockDropable, SpinLockGuard},
};

#[bitfield]
#[repr(u8)]
struct Command {
    channel: B2,
    access_mode: B2,
    operating_mode: B3,
    bcd: B1,
}

/// The PIT has three channels (0, 1, and 2) and a command register.
#[allow(dead_code)]
enum ChannelPort {
    Channel0 = 0x40,
    _Channel1 = 0x41,
    _Channel2 = 0x42,
    Command = 0x43,
}

/// The PIT has three channels (0, 1, and 2) and a command register.
#[allow(dead_code)]
enum Channel {
    Channel0 = 0b00,
    Channel1 = 0b01,
    Channel2 = 0b10,
    ReadBack = 0b11,
}

/// The available access modes for the PIT. The access mode determines how the channel port is
/// accessed.
#[allow(dead_code)]
enum AccessMode {
    Latch = 0b00,
    LowByte = 0b01,
    HighByte = 0b10,
    LowAndHighByte = 0b11,
}

/// The different operating modes of the PIT.
#[allow(dead_code)]
pub enum OperatingMode {
    /// On a `ChannelPort::Command` write the output signal turns 0, until the channel's reload
    /// register is written. When the reload register is written, the `current count` is reload on the next falling
    /// edge. Then, each falling edge decrements the `current count` by 1.
    /// During the switch between 1 and 0, the output signal is set to 1. And remains high and so forth
    InterruptOnTerminalCount = 0b000,
    /// Same as `InterruptOnTerminalCount`, but the counting starts on a rising edge instead (with
    /// a few other minor differences, but we don't care for this mode anyway)
    HardwareRetriggerableOneShot = 0b001,
    /// Essentially a frequency divider.
    /// On a `ChannelPort::Command` write the output signal turns 1, until the channel's reload.
    /// When the reload register is written, the `current count` is reload on the next falling
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

pub static PIT: SpinLock<Pit> = SpinLock::new(Pit {});

#[derive(Debug)]
pub struct Pit;

impl Pit {
    pub fn init(&mut self, period: Duration, operating_mode: OperatingMode) -> Result<(), PitError> {
        interrupts::do_inside_interrupts_disabled_window(|| -> Result<(), PitError> {
            let command = Command::new()
                .with_channel(Channel::Channel0 as u8)
                .with_access_mode(AccessMode::LowAndHighByte as u8)
                .with_operating_mode(operating_mode as u8)
                .with_bcd(false.into());

            let divisor = Pit::time_to_divisor(period)?;

            unsafe {
                self.write(command, divisor);
            }

            self.set_disabled(false);

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

    fn time_to_divisor(period: Duration) -> Result<u16, PitError> {
        Ok(10000)
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

unsafe impl SpinLockDropable for Pit {
    fn custom_unlock(&mut self) {
        self.set_disabled(true);
    }
}
