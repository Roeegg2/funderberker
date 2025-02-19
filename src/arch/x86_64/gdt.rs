use core::{arch::asm, cell::UnsafeCell, mem::size_of};

const GDT_ENTRIES_NUM: usize = 7;

//static GDT: UnsafeCell<[u64; GDT_ENTRIES_NUM]> = UnsafeCell::new([
//    0x0000_0000_0000_0000, // null
//    0x00af_9a00_0000_ffff, // ring 0 code: base=0x0, limit=0xfffff, flags=0xAF, access=0x9A
//    0x00af_9200_0000_ffff, // ring 0 data: base=0x0, limit=0xfffff, flags=0xAF, access=0x92
//    0x00af_fa00_0000_ffff, // ring 3 code: base=0x0, limit=0xfffff, flags=0xAF, access=0xFA
//    0x00af_f200_0000_ffff, // ring 3 data: base=0x0, limit=0xfffff, flags=0xAF, access=0xF2
//    0x0000_0000_0000_0000, // placeholder for TSS (Part 1)
//    0x0000_0000_0000_0000, // placeholder for TSS (Part 2)
//]);

static GDT: [u64; GDT_ENTRIES_NUM] = [
    0x0000_0000_0000_0000, // null
    0x00af_9a00_0000_ffff, // ring 0 code: base=0x0, limit=0xfffff, flags=0xAF, access=0x9A
    0x00af_9200_0000_ffff, // ring 0 data: base=0x0, limit=0xfffff, flags=0xAF, access=0x92
    0x00af_fa00_0000_ffff, // ring 3 code: base=0x0, limit=0xfffff, flags=0xAF, access=0xFA
    0x00af_f200_0000_ffff, // ring 3 data: base=0x0, limit=0xfffff, flags=0xAF, access=0xF2
    0x0000_0000_0000_0000, // placeholder for TSS (Part 1)
    0x0000_0000_0000_0000, // placeholder for TSS (Part 2)
];

pub(super) fn load_gdt() {
    let gdtr = super::DescriptorTablePtr {
        limit: (size_of::<[u64; GDT_ENTRIES_NUM]>() - 1) as u16,
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
            const 0x10, // ring 0 data
            const 0x08, // ring 0 code
            out(reg) _,
        );
    }
}
