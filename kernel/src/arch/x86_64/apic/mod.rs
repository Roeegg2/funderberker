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
    BusDefault = 0b0,
    ActiveHigh = 0b1,
    ActiveLow = 0b11,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum TriggerMode {
    BusDefault = 0b0,
    EdgeTriggered = 0b1,
    LevelTriggered = 0b11,
}

impl TryFrom<u16> for TriggerMode {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0b0 => Ok(TriggerMode::BusDefault),
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
            0b0 => Ok(PinPolarity::BusDefault),
            0b1 => Ok(PinPolarity::ActiveHigh),
            0b11 => Ok(PinPolarity::ActiveLow),
            _ => Err(()),
        }
    }
}
