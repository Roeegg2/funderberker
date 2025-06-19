//! Everything GDT and segments
//!
//! NOTE: Setting up the GDT only happens when we're booting on raw UEFI without any bootloaders.

use core::{arch::asm, mem::transmute, ops::Index, ptr};

use modular_bitfield::prelude::*;

use super::{DescriptorTablePtr, cpu::Register};

/// The "full" form of a segment selector (i.e. the actual selector + the hidden cached information)
#[derive(Default)]
#[repr(C, packed)]
pub struct FullSegmentSelector {
    pub selector: SegmentSelector,
    pub attributes: u16,
    pub limit: u32,
    pub base: u64,
}

/// A segment descriptor.
#[bitfield(bits = 64)]
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub struct SegmentDescriptor {
    pub limit_0: B16,
    pub base_0: B24,
    pub access: B8,
    pub limit_1: B4,
    pub flags: B4,
    pub base_1: B8,
}

// TODO: Add TSS here as well
/// The GDT
///
#[cfg(feature = "limine")]
#[repr(C, packed)]
pub struct Gdt {
    segments: [SegmentDescriptor; 6],
}

/// The basic, visible part of a segment selector.
#[bitfield(bits = 16)]
#[derive(Debug, Clone, Copy, Default)]
#[repr(u16)]
pub struct SegmentSelector {
    pub rpl: B2,
    pub gdt: B1,
    pub index: B13,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Cs(pub SegmentSelector);

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Ds(pub SegmentSelector);

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Ss(pub SegmentSelector);

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Es(pub SegmentSelector);

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fs(pub SegmentSelector);

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Gs(pub SegmentSelector);

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

    const _FLAGS_RESERVED: u8 = 1 << 4;
    const FLAGS_G: u8 = 1 << (4 + 1); // granuality
    const FLAGS_DB: u8 = 1 << (4 + 2); // size. 0-> 16 bit protected mode 1-> 32 bit protected
    const FLAGS_L: u8 = 1 << (4 + 3); // segment is 64 long mode. when set, DB shouldn't be

    #[inline]
    fn get_base(self) -> u32 {
        (u32::from(self.base_1()) << 24) | u32::from(self.base_0())
    }

    // TODO: Make this function const
    /// NOTE: The size of the limit is actually 20 bits, not 32
    #[inline]
    fn get_limit(self) -> u32 {
        (u32::from(self.limit_1()) << 16) | u32::from(self.limit_0())
    }
}

impl Default for SegmentDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for Cs {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, cs",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov cs, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
    }
}

impl Register for Ds {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, ds",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov ds, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
    }
}

impl Register for Gs {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, gs",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov gs, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
    }
}

impl Register for Fs {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, fs",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov fs, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
    }
}

impl Register for Es {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, es",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov es, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
    }
}

impl Register for Ss {
    #[inline]
    unsafe fn read() -> Self {
        let selector: u16;
        unsafe {
            asm!(
                "mov {:x}, ss",
                out(reg) selector,
                options(nostack, nomem),
            );
        }

        Self(SegmentSelector::from(selector))
    }

    #[inline]
    unsafe fn write(self) {
        unsafe {
            asm!(
                "mov ss, {:x}",
                in(reg) transmute::<Self, u16>(self),
                options(nostack, nomem),
            );
        }
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

#[cfg(test)]
mod tests {
    use core::mem::offset_of;
    use macros::test_fn;

    use crate::arch::x86_64::gdt::FullSegmentSelector;

    #[test_fn]
    fn test_full_segment_selector_layout() {
        assert_eq!(offset_of!(FullSegmentSelector, selector), 0);
        assert_eq!(offset_of!(FullSegmentSelector, attributes), 2);
        assert_eq!(offset_of!(FullSegmentSelector, limit), 4);
        assert_eq!(offset_of!(FullSegmentSelector, base), 8);
    }
}
