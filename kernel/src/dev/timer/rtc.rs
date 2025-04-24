
pub enum FrequencyRate {

}

pub struct Rtc {

}

impl Rtc {
    pub unsafe fn init() {
        // cli
        // write to CMOS with NMI disabled
        // read from CMOS
        // sti
        // write to CMOS with NMI enabled
    }

    pub fn new() -> Result<(), ()> {

    }

    const fn time_to_rate() -> FrequencyRate {

    }
}
