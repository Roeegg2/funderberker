//! Everything IDT and interrupts

use core::{
    arch::{asm, global_asm},
    mem::{size_of, transmute},
    ptr::from_ref,
};

use modular_bitfield::prelude::*;

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

/// Represents an entry in the(smute(0_u128) }))))))))))))))))))))))))))));
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
            IDT.load();

            #[allow(static_mut_refs)]
            IDT.install_init_isrs();
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

        self.0[InterruptVector::PageFault as usize].register(
            int_stub_14 as u64,
            cs,
            0,
            GateType::Trap,
            Dpl::Kernel,
            Present::Present,
        );
        self.0[InterruptVector::ProtectionFault as usize].register(
            int_stub_13 as u64,
            cs,
            0,
            GateType::Trap,
            Dpl::Kernel,
            Present::Present,
        );
        self.0[InterruptVector::Pit as usize].register(
            int_stub_32 as u64,
            cs,
            0,
            GateType::Interrupt,
            Dpl::Kernel,
            Present::Present,
        );
        self.0[InterruptVector::Unhandled as usize].register(
            int_stub_33 as u64,
            cs,
            0,
            GateType::Interrupt,
            Dpl::Kernel,
            Present::Present,
        );
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptVector {
    PageFault = 14,
    ProtectionFault = 13,
    Pit = 32,
    Unhandled = 33,
}

#[derive(Debug, Clone, Copy)]
pub enum Irq {
    Pit = 0,
    Unhandled = 1,
}

impl Irq {
    /// Get the vector for this IRQ
    pub const fn to_vector(self) -> InterruptVector {
        match self {
            Irq::Pit => InterruptVector::Pit,
            Irq::Unhandled => InterruptVector::Unhandled,
        }
    }
}

impl From<u8> for Irq {
    fn from(value: u8) -> Self {
        match value {
            0 => Irq::Pit,
            _ => Irq::Unhandled,
        }
    }
}

// Small stubs that redirect to the actual ISR handlers
global_asm! {
    r#"
    .section .text
    .macro define_int_stub int_id
    .global int_stub_\int_id
    int_stub_\int_id:
        call vec_int_\int_id
        iretq
    .endm

    define_int_stub 14
    define_int_stub 13
    define_int_stub 32
    define_int_stub 33
    "#
}

unsafe extern "C" {
    fn int_stub_14();
    fn int_stub_13();
    fn int_stub_32();
    fn int_stub_33();
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_13() {
    unsafe { asm!("hlt") };
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_14() {
    println!("got page fault! address: {:#x}", read_cr!(cr2));
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_32() {
    println!("GOT TIMER INTERRUPT!!!!");
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_33() {
    println!("unhandled interrupt received");
}
