//! Everything GDT and segments
//!
//! NOTE: Setting up the GDT only happens when we're booting on raw UEFI without any bootloaders.

use core::{arch::asm, ops::Index, ptr};

use modular_bitfield::prelude::*;

use super::DescriptorTablePtr;

/// The "full" form of a segment selector (i.e. the actual selector + the hidden cached information)
#[derive(Default)]
#[repr(C, packed)]
pub struct FullSegmentSelector {
    selector: SegmentSelector,
    attributes: u16,
    limit: u32,
    base: u64,
}

/// A segment descriptor.
#[bitfield]
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub struct SegmentDescriptor {
    limit_0: B16,
    base_0: B24,
    access: B8,
    limit_1: B4,
    flags: B4,
    base_1: B8,
}

/// The GDT
///
// TODO: Add TSS here as well
#[cfg(feature = "limine")]
#[repr(C, packed)]
pub struct Gdt {
    segments: [SegmentDescriptor; 6],
}

#[bitfield]
#[derive(Debug, Clone, Copy, Default)]
#[repr(u16)]
pub struct SegmentSelector {
    rpl: B2,
    gdt: B1,
    index: B13,
}

// /// The possible, valid, segment descriptors
// #[cfg(feature = "limine")]
// #[derive(Debug, Clone, Copy)]
// #[repr(u16)]
// pub enum SegmentSelectorIndex {
//     Code16 = 0,
//     Data16 = 1,
//     Code32 = 2,
//     Data32 = 3,
//     Code64 = 4,
//     Data64 = 5,
// }

impl Gdt {
    /// Get the address of the GDT, aquired by reading `GDTR`
    pub fn read_gdtr() -> DescriptorTablePtr {
        let gdtr = DescriptorTablePtr::default();
        unsafe {
            let ptr = ptr::from_ref(&gdtr);
            asm!(
                "sgdt [{}]",
                in(reg) ptr,
                options(nostack),
            );
        };

        gdtr
    }

    #[inline]
    pub fn read_full_selector(&self, selector: SegmentSelector) -> FullSegmentSelector {
        let descriptor = self[selector];

        FullSegmentSelector {
            selector,
            // NOTE: Not sure about this one, but it only seems to be the access byte from the source
            // code I've read
            attributes: descriptor.access() as u16,
            limit: descriptor.get_limit(),
            base: descriptor.get_base() as u64,
        }
    }
}

impl SegmentDescriptor {
    /// Accessed bit. Set to 1 by the CPU when accessed (unless set manually in advance)
    const ACCESS_A: u8 = 1 << 0;
    const ACCESS_RW: u8 = 1 << 1; // write acccess/read access for data/code
    const ACCESS_DC: u8 = 1 << 2; // direction/conforming for data/code
    const ACCESS_E: u8 = 1 << 3; // 0 -> data, 1 -> code
    const ACCESS_S: u8 = 1 << 4; // 0 -> system, 1 -> regular
    const ACCESS_DPL_3: u8 = 0b11 << 5;
    const ACCESS_DPL_2: u8 = 0b10 << 5;
    const ACCESS_DPL_1: u8 = 0b01 << 5;
    const ACCESS_P: u8 = 1 << 7; // present

    const _FLAGS_RESERVED: u8 = 1 << (4 + 0);
    const FLAGS_G: u8 = 1 << (4 + 1); // granuality
    const FLAGS_DB: u8 = 1 << (4 + 2); // size. 0-> 16 bit protected mode 1-> 32 bit protected
    const FLAGS_L: u8 = 1 << (4 + 3); // segment is 64 long mode. when set, DB shouldn't be

    #[inline]
    fn get_base(&self) -> u32 {
        (u32::from(self.base_1()) << 24) | u32::from(self.base_0())
    }

    // TODO: Make this function const
    /// NOTE: The size of the limit is actually 20 bits, not 32
    #[inline]
    fn get_limit(&self) -> u32 {
        (u32::from(self.limit_1()) << 16) | u32::from(self.limit_0())
    }
}

impl Index<SegmentSelector> for Gdt {
    type Output = SegmentDescriptor;

    fn index(&self, index: SegmentSelector) -> &Self::Output {
        let index = (index.index()) as usize >> 3;

        // XXX: This is true only for Limine!
        assert!(index <= 5);

        &self.segments[index]
    }
}

impl From<DescriptorTablePtr> for FullSegmentSelector {
    fn from(value: DescriptorTablePtr) -> Self {
        // NOTE: Not entirely sure about this
        Self {
            selector: SegmentSelector::new(),
            attributes: 0,
            limit: value.limit as u32,
            base: value.base as u64,
        }
    }
}
