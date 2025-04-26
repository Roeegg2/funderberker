//! APIC implementation

pub mod ioapic;
pub mod lapic;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DeliveryMode {
    Fixed = 0b000,
    LowestPriority = 0b001,
    Smi = 0b010,
    Nmi = 0b100,
    Init = 0b101,
    StartUp = 0b110,
    ExtInt = 0b111,
}

// TODO
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RemoteIrr {
    NotSet = 0b0,
    Set = 0b1,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Mask {
    Unmasked = 0b0,
    Masked = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PinPolarity {
    ActiveHigh = 0b0,
    ActiveLow = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum TriggerMode {
    EdgeTriggered = 0b0,
    LevelTriggered = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Level {
    Deassert = 0b0,
    Assert = 0b1,
}

/// The destination mode and an ID matching it's type
#[derive(Debug, Clone, Copy)]
pub enum Destination {
    Physical(u8),
    Logical(u8),
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DestinationShorthand {
    NoShorthand = 0b00,
    SelfDestination = 0b01,
    AllIncludingSelf = 0b10,
    AllExcludingSelf = 0b11,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum SharedFlags {
    ActiveHighEdgeTriggered = 0,
    ActiveHighLevelTriggered = 2,
    ActiveLowEdgeTriggered = 8,
    ActiveLowLevelTriggered = 2 | 8,
}

impl Destination {
    const PHYSICAL_MODE: u8 = 0b0;
    const LOGICAL_MODE: u8 = 0b1;

    /// Create the destination struct
    #[inline]
    pub const fn new(dest: u8, is_logical: bool) -> Result<Self, ()> {
        if is_logical {
            if dest | 0x0f != 0 {
                // TODO: Add error message here.
                // In this mode, the destination is a 4-bit logical destination ID.
                return Err(());
            }
            Ok(Destination::Logical(dest))
        } else {
            Ok(Destination::Physical(dest))
        }
    }

    /// Break the struct down into the destination mode and ID
    #[inline]
    pub const fn get(&self) -> (u8, u8) {
        match self {
            Destination::Physical(dest) => (Self::PHYSICAL_MODE, *dest),
            Destination::Logical(dest) => (Self::LOGICAL_MODE, *dest),
        }
    }
}

impl TryFrom<u16> for TriggerMode {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            // 0 is the bus default: on ISA, this is edge triggered
            0b0 => Ok(TriggerMode::EdgeTriggered),
            0b1 => Ok(TriggerMode::EdgeTriggered),
            0b11 => Ok(TriggerMode::LevelTriggered),
            _ => Err(()),
        }
    }
}

impl TryFrom<u16> for PinPolarity {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            // 0 is the bus default: on EISA, this is active low
            // XXX: Is this the default only for Level or for Edge as well?
            0b0 => Ok(PinPolarity::ActiveLow),
            0b1 => Ok(PinPolarity::ActiveHigh),
            0b11 => Ok(PinPolarity::ActiveLow),
            _ => Err(()),
        }
    }
}

impl TryFrom<u16> for SharedFlags {
    type Error = ();
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        const FOO: u16 = 8 | 2;
        match value {
            0 => Ok(SharedFlags::ActiveHighEdgeTriggered),
            2 => Ok(SharedFlags::ActiveHighLevelTriggered),
            8 => Ok(SharedFlags::ActiveLowEdgeTriggered),
            FOO => Ok(SharedFlags::ActiveLowLevelTriggered),
            _ => Err(()),
        }
    }
}
