use core::{arch::asm, mem::size_of};

static mut GDT: [u64; 7] = [
    0x0000_0000_0000_0000, // null
    0x00af_9a00_0000_ffff, // kernel code: base=0x0, limit=0xfffff, flags=0xAF, access=0x9A
    0x00af_9200_0000_ffff, // kernel data: base=0x0, limit=0xfffff, flags=0xAF, access=0x92
    0x00af_fa00_0000_ffff, // user code: base=0x0, limit=0xfffff, flags=0xAF, access=0xFA
    0x00af_f200_0000_ffff, // user data: base=0x0, limit=0xfffff, flags=0xAF, access=0xF2
    0x0000_0000_0000_0000, // placeholder for TSS (Part 1)
    0x0000_0000_0000_0000, // placeholder for TSS (Part 2)
];

pub(super) fn load_gdt() {
    let gdtr = super::DescriptorTablePtr {
        limit: (size_of::<[u64; 7]>() - 1) as u16,
        base: &raw const GDT as u64,
    };

    // TODO: actually set TSS

    unsafe {
        asm!(
            "lgdt [{0}]", // load gdt
            "mov ax, {1}", // reloading data segments
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            "push {2}", // push new CS selector
            "lea {3}, [2f]",
            "push {3}",
            "retfq", // execute retfq to reload CS
            "2:", // redundant label
            in(reg) &gdtr,
            const 0x10, // kernel data
            const 0x08, // kernel code
            out(reg) _,
        );
    }
}
