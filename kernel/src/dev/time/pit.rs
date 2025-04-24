use core::time::Duration;

use modular_bitfield::prelude::*;

use crate::{
    arch::x86_64::{
        apic::ioapic::{IoApic, IO_APICS},
        cpu::{self, cli, inb_8, outb_8, sti},
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
    Channel1 = 0x41,
    Channel2 = 0x42,
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
enum OperatingMode {
    InterruptOnTerminalCount = 0b000,
    HardwareRetriggerableOneShot = 0b001,
    RateGenerator = 0b010,
    SquareWaveGenerator = 0b011,
    SoftwareTriggeredStrobe = 0b100,
    HardwareTriggeredStrobe = 0b101,
    _RateGenerator2 = 0b110,
    _SquareWaveGenerator2 = 0b111,
}

// TODO: Move this somewhere else!
const PIT_IRQ: u8 = 0;
const PIT_VECTOR: u8 = 0x20;

pub static PIT: SpinLock<Pit> = SpinLock::new(Pit::new(0));

#[derive(Debug)]
pub struct Pit {
    ioapic_id: u8,
}

impl Pit {
    const fn new(ioapic_id: u8) -> Self {
        Pit { ioapic_id }
    }

    pub unsafe fn init() {
        // XXX: Not sure I need to lock here, since no threads should be initialized yet
        let mut pit = PIT.lock();

        // let ioapic_id = IoApic::find_ioapic_id(PIT_IRQ);
        let ioapic_id = 0;

        pit.ioapic_id = ioapic_id;
        unsafe { pit.set_status(false) };

        // Interrupts were disabled on x86_64::init(), so it doesn't interfere with the boot
        // process.
        // But now we need to enable it so the PIT can send interrupts.
        //
        // SAFETY: It's important to do this **after** disabling the PIT, otherwise the PIT could
        // theoretically send interrupts while we're trying to set it up; We don't know it's boot
        // state (Slim chance, but nonetheless possible)
        unsafe {
            cpu::sti();
        }
    }

    #[inline]
    unsafe fn set_status(&mut self, status: bool) {
        unsafe {
            IO_APICS[self.ioapic_id as usize].set_irq_status(PIT_IRQ.into(), status);
        }
    }

    fn time_to_divisor(period: Duration) -> Result<u16, ()> {
        const BASE_FREQ: u64 = 1193182; // 1.193182 MHz

        let frequency = 1_000_000 / period.as_micros() as u64;
        if frequency > BASE_FREQ {
            return Err(());
        }

        let divisor = BASE_FREQ / frequency;
        if divisor > 0xFFFF {
            return Err(());
        }

        Ok(divisor as u16)
    }

    unsafe fn write(&mut self, command: Command, divisor: u16) {
        unsafe {
            let val: u8 = command.into();
            outb_8(ChannelPort::Command as u16, val);

            outb_8(ChannelPort::Channel0 as u16, (divisor & 0xff) as u8);
            outb_8(ChannelPort::Channel0 as u16, ((divisor >> 8) & 0xff) as u8);
        };
    }

    pub fn new_periodic(&mut self, period: Duration) -> Result<(), ()> {
        let command = Command::new()
            .with_channel(Channel::Channel0 as u8)
            .with_access_mode(AccessMode::LowAndHighByte as u8)
            .with_operating_mode(OperatingMode::RateGenerator as u8)
            .with_bcd(0b0);

        let divisor = Pit::time_to_divisor(period)?;

        unsafe {
            self.write(command, divisor);

            self.set_status(true);
        }

        Ok(())
    }

    pub fn read_count(&mut self) -> u16 {
        let mut count: u16;
        unsafe {
            let command = Command::new()
                .with_channel(Channel::ReadBack as u8)
                .with_access_mode(0b00)
                .with_operating_mode(0b000)
                .with_bcd(0b0);

            self.write(command, 0);

            count = inb_8(ChannelPort::Channel0 as u16) as u16;
            count |= (inb_8(ChannelPort::Channel0 as u16) as u16) << 8;
        }

        count
    }
}

impl SpinLockDropable for Pit {
    unsafe fn custom_unlock(&mut self) {
        println!("Unlocking PIT");
        unsafe {
            self.set_status(false);
        }
    }
}
