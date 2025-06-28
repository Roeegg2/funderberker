//! Everything IDT and interrupts

use crate::arch::x86_64::{
    cpu::{self, Register},
    event::GENERIC_ISR_VECTOR,
    gdt::Cs,
};
use core::{
    arch::asm,
    mem::{size_of, transmute},
    ptr::{self, from_ref},
};
use modular_bitfield::prelude::*;
use utils::sync::spinlock::{SpinLock, SpinLockable};

use super::{
    DescriptorTablePtr,
    apic::ioapic::{self, map_irq_to_vector, set_disabled},
    gdt::SegmentSelector,
};

/// The number of entries in the IDT
const IDT_ENTRIES_NUM: usize = 256;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// The IDT
static IDT: SpinLock<Idt> = SpinLock::new(Idt([GateDescriptor::DEFAULT; IDT_ENTRIES_NUM]));

/// The IDT
pub struct Idt([GateDescriptor; IDT_ENTRIES_NUM]);

/// An ISR stub
///
/// NOTE: That's not the actual ISR, that's only stub.
pub type IsrStub = unsafe extern "C" fn();

#[bitfield(bits = 128)]
#[derive(Debug, Clone, Copy)]
#[repr(u128)]
/// A gate descriptor instance. These are the entries of the IDT
struct GateDescriptor {
    offset_0: B16,
    segment_selector: B16,
    ist: B3,
    reserved_0: B5,
    gate_type: B4,
    zero: B1,
    dpl: B2,
    present: B1,
    offset_1: B16,
    offset_2: B32,
    reserved_1: B32,
}

/// The type of the gate
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum GateType {
    /// This IDT entry is for an interrupt
    Interrupt = 0b1110,
    /// This IDT entry is a trap (ie exception)
    Trap = 0b1111,
}

/// The minimum privilege level that the CPU must be in in order to trigger this ISR using an `int`
/// instruction (as such it's ignored by hardware IRQs)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dpl {
    /// Ring 0 privilge
    Kernel = 0b00,
    /// Ring 3
    User = 0b11,
}

/// Marks the entry as present/no present. If an entry isn't present a PF will be triggered
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Present {
    /// Entry is not present
    NotPresent = 0,
    /// Entry is presnet
    Present = 1,
}

/// Represents an entry in the IDT
impl GateDescriptor {
    const DEFAULT: Self = unsafe { transmute(0_u128) };

    /// Registers the entry in the IDT with the given parameters.
    fn install(
        &mut self,
        offset: u64,
        segment_selector: SegmentSelector,
        ist: u8,
        gate_type: GateType,
        dpl: Dpl,
        present: Present,
    ) {
        self.set_offset_0(offset as u16);
        self.set_segment_selector(segment_selector.into());
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
    /// NOTE: THIS FUNCTION SHOULD BE CALLED ONLY ONCE DURING BOOT
    pub(super) fn init() {
        cpu::cli();
        let mut idt = IDT.lock();

        unsafe {
            idt.install_exception_isrs();

            idt.load();
        };

        cpu::sti();
    }

    /// Install all the exception ISR handlers
    #[inline]
    #[rustfmt::skip]
    fn install_exception_isrs(&mut self) {
        let cs = unsafe { Cs::read().0 };

        self.0[0].install(__isr_stub_exception_0 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[1].install(__isr_stub_exception_1 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[2].install(__isr_stub_exception_2 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[3].install(__isr_stub_exception_3 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[4].install(__isr_stub_exception_4 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[5].install(__isr_stub_exception_5 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[6].install(__isr_stub_exception_6 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[7].install(__isr_stub_exception_7 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[8].install(__isr_stub_exception_8 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[9].install(__isr_stub_exception_9 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[10].install(__isr_stub_exception_10 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[11].install(__isr_stub_exception_11 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[12].install(__isr_stub_exception_12 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[13].install(__isr_stub_exception_13 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[14].install(__isr_stub_exception_14 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[15].install(__isr_stub_exception_15 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[16].install(__isr_stub_exception_16 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[17].install(__isr_stub_exception_17 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[18].install(__isr_stub_exception_18 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[19].install(__isr_stub_exception_19 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[20].install(__isr_stub_exception_20 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[21].install(__isr_stub_exception_21 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[22].install(__isr_stub_exception_22 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[23].install(__isr_stub_exception_23 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[24].install(__isr_stub_exception_24 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[25].install(__isr_stub_exception_25 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[26].install(__isr_stub_exception_26 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[27].install(__isr_stub_exception_27 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[28].install(__isr_stub_exception_28 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[29].install(__isr_stub_exception_29 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[30].install(__isr_stub_exception_30 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.0[31].install(__isr_stub_exception_31 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);

        self.0[GENERIC_ISR_VECTOR as usize].install(__isr_stub_generic_irq_isr as usize as u64, cs, 0, GateType::Interrupt, Dpl::Kernel, Present::Present);

        logger::info!("Installed ISRs successfully");
    }

    /// Loads the IDT into memory.
    ///
    /// NOTE: THIS FUNCTION SHOULD BE CALLED ONLY ONCE DURING BOOT! from `Idt::init()`
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
            );
        }

        logger::info!("Loaded IDT successfully");
    }

    /// Get the address of the IDTR, aquired by reading the `IDTR`
    pub fn read_idtr() -> DescriptorTablePtr {
        let idtr = DescriptorTablePtr::default();
        unsafe {
            let ptr = ptr::from_ref(&idtr);
            asm!(
                "sidt [{}]",
                in(reg) ptr,
                options(nostack),
            );
        };

        idtr
    }
}

/// Find an available entry in the IDT and install an ISR entry there
///
/// NOTE: Make sure to call with the *ISR stub* and *not the actual handler!!* (ie. `__isr_stub_..`)
pub unsafe fn install_isr(
    isr_stub: IsrStub,
    segment_selector: SegmentSelector,
    ist_field: u8,
    gate_type: GateType,
    dpl: Dpl,
    present: Present,
) -> u8 {
    let mut idt = IDT.lock();

    let (entry_number, entry) = idt
        .0
        .iter_mut()
        .enumerate()
        .find(|entry| entry.1.present() == Present::NotPresent as u8)
        .unwrap();

    entry.install(
        isr_stub as usize as u64,
        segment_selector,
        ist_field,
        gate_type,
        dpl,
        present,
    );

    entry_number as u8
}

// TODO: Return an error instead of panicking here
/// A wrapper for easier installing of IRQ ISRs
pub unsafe fn register_irq(irq: u8, isr_stub: IsrStub) {
    unsafe {
        // Make sure the interrupt is masked off before we do any fiddiling with the
        // IO APIC and IDT
        ioapic::set_disabled(irq, true).unwrap();

        // Install the new ISR
        let vector = install_isr(
            isr_stub,
            Cs::read().0,
            0,
            GateType::Interrupt,
            Dpl::Kernel,
            Present::Present,
        );

        // Tell the IO APIC to map `irq` to the given `vector`
        // XXX: Change the flags here!
        map_irq_to_vector(vector, irq).unwrap();

        // Now we can unmask the IRQ in the IO APIC
        //
        // NOTE: No interrupt should be triggered yet, since the timer is still
        // disabled internally.
        set_disabled(irq, false).unwrap();
    };
}

// TODO: unregister_isr

unsafe extern "C" {
    fn __isr_stub_exception_0();
    fn __isr_stub_exception_1();
    fn __isr_stub_exception_2();
    fn __isr_stub_exception_3();
    fn __isr_stub_exception_4();
    fn __isr_stub_exception_5();
    fn __isr_stub_exception_6();
    fn __isr_stub_exception_7();
    fn __isr_stub_exception_8();
    fn __isr_stub_exception_9();
    fn __isr_stub_exception_10();
    fn __isr_stub_exception_11();
    fn __isr_stub_exception_12();
    fn __isr_stub_exception_13();
    fn __isr_stub_exception_14();
    fn __isr_stub_exception_15();
    fn __isr_stub_exception_16();
    fn __isr_stub_exception_17();
    fn __isr_stub_exception_18();
    fn __isr_stub_exception_19();
    fn __isr_stub_exception_20();
    fn __isr_stub_exception_21();
    fn __isr_stub_exception_22();
    fn __isr_stub_exception_23();
    fn __isr_stub_exception_24();
    fn __isr_stub_exception_25();
    fn __isr_stub_exception_26();
    fn __isr_stub_exception_27();
    fn __isr_stub_exception_28();
    fn __isr_stub_exception_29();
    fn __isr_stub_exception_30();
    fn __isr_stub_exception_31();

    fn __isr_stub_generic_irq_isr();
}

impl SpinLockable for Idt {}
