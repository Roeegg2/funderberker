//! Everything IDT and interrupts
use core::{arch::{asm, global_asm}, mem::size_of, ptr::from_ref};

const IDT_ENTRIES_NUM: usize = 256;

// TODO: Definitely use an UnsafeCell with some locking mechanism here
/// The IDT
static mut IDT: Idt = Idt([GateDescriptor::DEFAULT; IDT_ENTRIES_NUM]);

/// The IDT 
pub(super) struct Idt([GateDescriptor; IDT_ENTRIES_NUM]);

/// Represents an entry in the IDT.
#[repr(C, packed)]
#[derive(Debug)]
struct GateDescriptor {
    offset_0: u16,
    segment_selector: u16,
    ist_n_reserved: u8,
    gate_n_zero_n_dpl_n_p: u8,
    offset_1: u16,
    offset_2: u32,
    _reserved: u32,
}

impl GateDescriptor {
    /// Register an interrupt handler in this gate descriptor
    const fn register(&mut self, offset: u64, selector: u16, ist: u8, gate: u8, dpl: u8, p: u8) {
        self.offset_0 = (offset & 0xffff) as u16;
        self.segment_selector = selector;
        self.ist_n_reserved = ist & 0b111;
        self.gate_n_zero_n_dpl_n_p = (gate & 0xf) | (p << 7) | ((dpl & 0b11) << 5);
        self.offset_1 = ((offset & 0xffff_0000) >> 16) as u16;
        self.offset_2 = ((offset & 0xffff_ffff_0000_0000) >> 32) as u32;
        self._reserved = 0;
    }

    /// Default initial value of each gate descriptor
    const DEFAULT: Self = Self {
        offset_0: 0,
        segment_selector: 0,
        ist_n_reserved: 0,
        gate_n_zero_n_dpl_n_p: 0,
        offset_1: 0,
        offset_2: 0,
        _reserved: 0,
    };
}

impl Idt {
    /// Loads the IDT into memory.
    ///
    /// NOTE: This function should be called ONLY ONCE DURING BOOT!
    /// NOTE: Must make sure there is a valid working GDT already loaded
    pub(super) unsafe fn init() {
        unsafe {
            #[allow(static_mut_refs)]
            IDT.load()
        };
        install_isr_handlers();
        log_info!("Installed ISRs successfully");
    }

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

/// Installs the ISR handlers in the GDT
#[inline]
fn install_isr_handlers() {
    // read the value of CS
    let cs: u16;
    unsafe {
        asm! (
            "mov {:x}, cs",
            out(reg) cs,
        )
    }

    unsafe {
        // page fault handler
        IDT.0[14].register(int_stub_14 as u64, cs, 0, 0b1111, 0, 1);
        // protection fault handler
        IDT.0[13].register(int_stub_13 as u64, cs, 0, 0b1111, 0, 1);
    };
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
    "#
}

unsafe extern "C" {
    fn int_stub_14();
    fn int_stub_13();
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_13() {
    unsafe { asm!("hlt") };
}

#[unsafe(no_mangle)]
extern "C" fn vec_int_14() {
    println!("got page fault! address: {:#x}", read_cr!(cr2));
}
