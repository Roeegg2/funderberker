//! Everything IDT and interrupts

use core::{
    arch::asm,
    mem::{size_of, transmute},
    ptr::from_ref,
};

use modular_bitfield::prelude::*;

use super::cpu::{cli, sti};

/// The number of entries in the IDT
const IDT_ENTRIES_NUM: usize = 256;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// The IDT
static mut IDT: Idt = Idt([GateDescriptor::DEFAULT; IDT_ENTRIES_NUM]);

/// The IDT
pub(super) struct Idt([GateDescriptor; IDT_ENTRIES_NUM]);

#[bitfield]
#[derive(Debug, Clone, Copy)]
#[repr(u128)]
/// Gate descriptor for the IDT
struct GateDescriptor {
    offset_0: B16,
    segment_selector: B16,
    ist: B3,
    _reserved_0: B5,
    gate_type: B4,
    zero: B1,
    dpl: B2,
    present: B1,
    offset_1: B16,
    offset_2: B32,
    _reserved_1: B32,
}

#[allow(dead_code)]
enum GateType {
    Interrupt = 0b1110,
    Trap = 0b1111,
}

#[allow(dead_code)]
enum Dpl {
    Kernel = 0b00,
    User = 0b11,
}

#[allow(dead_code)]
enum Present {
    NotPresent = 0,
    Present = 1,
}

/// Represents an entry in the IDT
impl GateDescriptor {
    const DEFAULT: Self = unsafe { transmute(0_u128) };

    fn register(
        &mut self,
        offset: u64,
        segment_selector: u16,
        ist: u8,
        gate_type: GateType,
        dpl: Dpl,
        present: Present,
    ) {
        self.set_offset_0(offset as u16);
        self.set_segment_selector(segment_selector);
        self.set_ist(ist);
        self.set_gate_type(gate_type as u8);
        self.set_dpl(dpl as u8);
        self.set_present(present as u8);
        self.set_offset_1((offset >> 16) as u16);
        self.set_offset_2((offset >> 32) as u32);
    }
}

impl Idt {
    /// Initializes the IDT.
    ///
    /// NOTE: This function should be called ONLY ONCE DURING BOOT!
    /// NOTE: Must make sure there is a valid working GDT already loaded
    pub(super) unsafe fn init() {
        unsafe {
            #[allow(static_mut_refs)]
            IDT.install_init_isrs();

            #[allow(static_mut_refs)]
            IDT.load();
        };

        log_info!("Installed ISRs successfully");
    }

    /// Loads the IDT into memory.
    ///
    /// NOTE: This function should be called ONLY ONCE DURING BOOT! from `Idt::init()`
    unsafe fn load(&mut self) {
        let idtr = super::DescriptorTablePtr {
            base: from_ref(self).addr() as u64,
            limit: (size_of::<Idt>() - 1) as u16,
        };

        // Load the IDTR
        unsafe {
            asm! (
                "lidt [{}]",
                in(reg) &idtr,
            )
        }

        log_info!("Loaded IDT successfully");
    }

    /// Installs the ISR handlers in the GDT
    #[inline]
    fn install_init_isrs(&mut self) {
        // read the value of CS
        let cs: u16;
        unsafe {
            asm! (
                "mov {:x}, cs",
                out(reg) cs,
            )
        }

        #[rustfmt::skip]
        {
            self.0[0].register(stub_vec_0 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[1].register(stub_vec_1 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[2].register(stub_vec_2 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[3].register(stub_vec_3 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[4].register(stub_vec_4 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[5].register(stub_vec_5 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[6].register(stub_vec_6 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[7].register(stub_vec_7 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[8].register(stub_vec_8 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[9].register(stub_vec_9 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[10].register(stub_vec_10 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[11].register(stub_vec_11 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[12].register(stub_vec_12 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[13].register(stub_vec_13 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[14].register(stub_vec_14 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[15].register(stub_vec_15 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[16].register(stub_vec_16 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[17].register(stub_vec_17 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[18].register(stub_vec_18 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[19].register(stub_vec_19 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[20].register(stub_vec_20 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[21].register(stub_vec_21 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[22].register(stub_vec_22 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[23].register(stub_vec_23 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[24].register(stub_vec_24 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[25].register(stub_vec_25 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[26].register(stub_vec_26 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[27].register(stub_vec_27 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[28].register(stub_vec_28 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[29].register(stub_vec_29 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[30].register(stub_vec_30 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
            self.0[31].register(stub_vec_31 as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);

            self.0[32].register(stub_vec_32 as u64, cs, 0, GateType::Interrupt, Dpl::Kernel, Present::Present);
            self.0[33].register(stub_vec_33 as u64, cs, 0, GateType::Interrupt, Dpl::Kernel, Present::Present);
            self.0[34].register(stub_vec_34 as u64, cs, 0, GateType::Interrupt, Dpl::Kernel, Present::Present);
            self.0[254].register(stub_vec_254 as u64, cs, 0, GateType::Interrupt, Dpl::Kernel, Present::Present);
        }
    }
}

unsafe extern "C" {
    fn stub_vec_0();
    fn stub_vec_1();
    fn stub_vec_2();
    fn stub_vec_3();
    fn stub_vec_4();
    fn stub_vec_5();
    fn stub_vec_6();
    fn stub_vec_7();
    fn stub_vec_8();
    fn stub_vec_9();
    fn stub_vec_10();
    fn stub_vec_11();
    fn stub_vec_12();
    fn stub_vec_13();
    fn stub_vec_14();
    fn stub_vec_15();
    fn stub_vec_16();
    fn stub_vec_17();
    fn stub_vec_18();
    fn stub_vec_19();
    fn stub_vec_20();
    fn stub_vec_21();
    fn stub_vec_22();
    fn stub_vec_23();
    fn stub_vec_24();
    fn stub_vec_25();
    fn stub_vec_26();
    fn stub_vec_27();
    fn stub_vec_28();
    fn stub_vec_29();
    fn stub_vec_30();
    fn stub_vec_31();

    fn stub_vec_32();
    fn stub_vec_33();
    fn stub_vec_34();
    fn stub_vec_254();
}

pub type Irq = u8;

pub type InterruptVector = u8;

pub const PIT_IRQ: u8 = 0x0;
pub const RTC_IRQ: u8 = 0x8;

/// Convert an IRQ to the matching interrupt vector
pub const fn irq_to_vector(irq: Irq) -> InterruptVector {
    match irq {
        PIT_IRQ => 33,
        RTC_IRQ => 34,
        _ => 254,
    }
}

/// Check if the `CLI` flag is set
pub fn check_interrupts_disabled() -> bool {
    let flags: u64;
    unsafe {
        asm!(
            "pushfq",
            "pop {flags}",
            flags = out(reg) flags,
        )
    }
    (flags & 0x200) == 0
}

pub fn do_inside_interrupts_disabled_window<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    let old = check_interrupts_disabled();
    cli();
    let ret = f();

    if !old {
        sti();
    }

    ret
}
