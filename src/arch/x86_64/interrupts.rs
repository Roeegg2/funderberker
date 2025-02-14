use core::{arch::asm, mem::size_of};

static mut IDT: [GateDescriptor; 255] = [GateDescriptor::DEFAULT; 255];

#[derive(Debug)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
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
    const fn register(&mut self, offset: u64, selector: u16, ist: u8, gate: u8, dpl: u8, p: u8) {
        self.offset_0 = (offset & 0xffff) as u16;
        self.segment_selector = selector;
        self.ist_n_reserved = ist & 0b111;
        self.gate_n_zero_n_dpl_n_p = (gate & 0xf) | (p << 7) | ((dpl & 0b11) << 5);
        self.offset_1 = ((offset & 0xffff_0000) >> 16) as u16;
        self.offset_2 = ((offset & 0xffff_ffff_0000_0000) >> 32) as u32;
        self._reserved = 0;
    }

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

pub(super) fn load_idt() {
    let idtr = super::DescriptorTablePtr {
        base: &raw const IDT as *const [GateDescriptor; 255] as u64, // address of IDT
        limit: (size_of::<[GateDescriptor; 255]>() - 1) as u16,      // size of IDT -1
    };

    unsafe {
        asm! (
            "lidt [{}]",
            in(reg) &idtr,
        )
    }

    // setup the ISR handlers
    install_isr_handlers();
    #[cfg(debug_assertions)]
    test_interrupts();
}

fn test_interrupts() {
    unsafe { asm!("int 38",) }
}

fn install_isr_handlers() {
    unsafe { IDT[38].register(interrupt_handler as u64, 0x8, 0, 0b1110, 0, 1) };
}

#[naked]
#[unsafe(no_mangle)]
pub extern "C" fn interrupt_handler() {
    unsafe {
        core::arch::naked_asm!(
            "cli",      // Disable interrupts
            "push rax", // Save registers
            "push rcx",
            "push rdx",
            "push rbx",
            "push rsp",
            "push rbp",
            "push rsi",
            "push rdi",
            "call handle_interrupt", // Call Rust function
            "pop rdi",               // Restore registers
            "pop rsi",
            "pop rbp",
            "pop rsp",
            "pop rbx",
            "pop rdx",
            "pop rcx",
            "pop rax",
            "sti",   // Re-enable interrupts
            "iretq", // Return from interrupt
        );
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_interrupt() {
    println!("GOT INTERRUPT {}!!", 38);
}
