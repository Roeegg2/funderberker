//! Everything IDT and interrupts

use crate::{
    arch::x86_64::cpu,
    sync::spinlock::{SpinLock, SpinLockDropable},
};
use core::{
    arch::asm,
    mem::{size_of, transmute},
    ptr::from_ref,
};
use modular_bitfield::prelude::*;
use utils::id_allocator::{Id, IdAllocator};

/// The number of entries in the IDT
const IDT_ENTRIES_NUM: usize = 256;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// The IDT
static IDT: SpinLock<Idt> = SpinLock::new(Idt {
    entries: [GateDescriptor::DEFAULT; IDT_ENTRIES_NUM],
    entry_tracker: IdAllocator::uninit(),
});

/// The IDT
pub(super) struct Idt {
    /// The actual IDT
    entries: [GateDescriptor; IDT_ENTRIES_NUM],
    /// A simply ID allocator to keep track of the used/unused entries
    entry_tracker: IdAllocator,
}

#[bitfield]
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
enum GateType {
    /// This IDT entry is for an interrupt
    Interrupt = 0b1110,
    /// This IDT entry is a trap (ie exception)
    Trap = 0b1111,
}

/// The minimum privilege level that the CPU must be in in order to trigger this ISR using an `int`
/// instruction (as such it's ignored by hardware IRQs)
#[allow(dead_code)]
enum Dpl {
    /// Ring 0 privilge
    Kernel = 0b00,
    /// Ring 3
    User = 0b11,
}

/// Marks the entry as present/no present. If an entry isn't present a PF will be triggered
#[allow(dead_code)]
enum Present {
    /// Entry is not present
    NotPresent = 0,
    /// Entry is presnet
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
    /// NOTE: THIS FUNCTION SHOULD BE CALLED ONLY ONCE DURING BOOT
    pub(super) fn init() {
        cpu::cli();
        let mut idt = IDT.lock();

        // Setup the entry tracker
        idt.entry_tracker = IdAllocator::new(Id(0)..Id(255));
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
        let cs = cpu::get_cs();

        self.entries[0].register(__isr_stub_exception_0 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[1].register(__isr_stub_exception_1 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[2].register(__isr_stub_exception_2 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[3].register(__isr_stub_exception_3 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[4].register(__isr_stub_exception_4 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[5].register(__isr_stub_exception_5 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[6].register(__isr_stub_exception_6 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[7].register(__isr_stub_exception_7 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[8].register(__isr_stub_exception_8 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[9].register(__isr_stub_exception_9 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[10].register(__isr_stub_exception_10 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[11].register(__isr_stub_exception_11 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[12].register(__isr_stub_exception_12 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[13].register(__isr_stub_exception_13 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[14].register(__isr_stub_exception_14 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[15].register(__isr_stub_exception_15 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[16].register(__isr_stub_exception_16 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[17].register(__isr_stub_exception_17 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[18].register(__isr_stub_exception_18 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[19].register(__isr_stub_exception_19 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[20].register(__isr_stub_exception_20 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[21].register(__isr_stub_exception_21 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[22].register(__isr_stub_exception_22 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[23].register(__isr_stub_exception_23 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[24].register(__isr_stub_exception_24 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[25].register(__isr_stub_exception_25 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[26].register(__isr_stub_exception_26 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[27].register(__isr_stub_exception_27 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[28].register(__isr_stub_exception_28 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[29].register(__isr_stub_exception_29 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[30].register(__isr_stub_exception_30 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);
        self.entries[31].register(__isr_stub_exception_31 as usize as u64, cs, 0, GateType::Trap, Dpl::Kernel, Present::Present);

        // Mark the entries as taken
        for i in 0..=31 {
            self.entry_tracker.allocate_at(Id(i)).unwrap();
        }

        log_info!("Installed ISRs successfully");
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
            )
        }

        log_info!("Loaded IDT successfully");
    }
}

/// Find an available entry in the IDT and install an ISR entry there
///
/// NOTE: Make sure to call with the *ISR stub* and *not the actual handler!!* (ie. `__isr_stub_..`)
pub unsafe fn install_isr(
    isr_stub: extern "C" fn(),
    segment_selector: u16,
    ist: u8,
    gate_type: GateType,
    dpl: Dpl,
    present: Present,
) -> Result<Id, ()> {
    let mut idt = IDT.lock();

    let index = idt.entry_tracker.allocate().map_err(|_| ())?;

    idt.entries[index.0].register(
        isr_stub as usize as u64,
        segment_selector,
        ist,
        gate_type,
        dpl,
        present,
    );

    Ok(index)
}

/// Install an ISR entry at the given `index` if it's free. Otherwise return
///
/// NOTE: Make sure to call with the *ISR stub* and *not the actual handler!!* (ie. `__isr_stub_..`)
pub unsafe fn install_isr_at(
    isr_stub: extern "C" fn(),
    segment_selector: u16,
    ist: u8,
    gate_type: GateType,
    dpl: Dpl,
    present: Present,
    index: usize,
) -> Result<(), ()> {
    assert!(index < 256);
    let mut idt = IDT.lock();

    idt.entry_tracker.allocate_at(Id(index)).map_err(|_| ())?;

    idt.entries[index].register(
        isr_stub as usize as u64,
        segment_selector,
        ist,
        gate_type,
        dpl,
        present,
    );

    Ok(())
}

/// Tries to uninstall the ISR at the given `index`, and disables it. An error is returned if the ISR isn't already
/// allocated
pub unsafe fn uninstall_isr(index: Id) -> Result<(), ()> {
    assert!(index.0 < 256);

    let mut idt = IDT.lock();

    unsafe {
        // Release the IDT entry
        idt.entry_tracker.free(index).map_err(|_| ())?;

        // Mark the entry as not present
        idt.entries[index.0].set_present(Present::NotPresent as u8);
    }

    Ok(())
}

/// Mark the entry at the given `index` with the given `Present` value.
/// If the index is out of bounds or the entry isn't allocated, an error is returned.
pub unsafe fn set_entry_present(index: Id, present: Present) -> Result<(), ()> {
    assert!(index.0 < 256);

    let mut idt = IDT.lock();

    // Make sure the entry is indeed allocated
    if idt.entry_tracker.allocate_at(index).is_ok() {
        return Err(());
    }

    idt.entries[index.0].set_present(present as u8);

    Ok(())
}

impl SpinLockDropable for Idt {}

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
}
