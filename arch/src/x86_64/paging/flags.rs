use core::marker::PhantomData;

use crate::{paging::Flags, x86_64::X86_64};

impl Flags<X86_64> {
    /// Keep all flags as off
    pub const FLAGS_NONE: usize = 0;
    /// Present bit (`0`):
    /// - If `1` the page is present.
    /// - If `0` the page isn't present, and accessing it would cause a PF
    pub(super) const FLAG_P: usize = 1 << 0;
    /// Read/write bit (`1`):
    /// - If `1` any address this page maps is writeable and readable
    /// - If `0` any address this page maps is read-only
    pub(super) const FLAG_RW: usize = 1 << 1;
    /// User/supervisor bit (`2`):
    /// - If `1` any address this page maps is accessible from user mode
    /// - If `0` any address this page maps is accessible only from kernel mode
    pub(super) const FLAG_US: usize = 1 << 2;
    /// Page-level write-through bit (`3`):
    /// - If `1` the page is write-through
    /// - If `0` the page is write-back
    pub(super) const FLAG_PWT: usize = 1 << 3;
    /// Page-level cache disable bit (`4`):
    /// - If `1` the page is not cacheable
    /// - If `0` the page is cacheable
    pub(super) const FLAG_PCD: usize = 1 << 4;
    /// Accessed bit (`5`):
    /// - If `1` the page has been accessed (read from or written to)
    /// - If `0` the page has not been accessed
    pub(super) const FLAG_A: usize = 1 << 5;
    /// Dirty bit (`6`):
    /// - If `1` the page has been written to
    /// - If `0` the page has not been written to
    pub(super) const FLAG_D: usize = 1 << 6;
    /// Page size bit (`7`):
    /// - If `1` the page is 2MB
    /// - If `0` the page is 4KB
    pub(super) const FLAG_PS: usize = 1 << 7;
    /// Global bit (`8`):
    /// - If `1` the CPU won't update the associated address when the TLB is flushed
    /// - If `0` the CPU will update the associated address when the TLB is flushed
    pub(super) const FLAG_G: usize = 1 << 8;
    /// Ignored bits (`9-11`) on AMD:
    /// Ignored bits (`9-10`) on Intel:
    pub(super) const RESERVED: usize = 1 << 9;
    /// HLAT paging bit (`12` Intel ONLY!!):
    pub(super) const HLAT: usize = 1 << 11;
    /// PAT bit (`12`) (on PDPE!)
    pub(super) const FLAG_1GB_PAT: usize = 1 << 12;
    /// PAT bit (`7`) (on PTE!)
    pub(super) const FLAG_4KB_PAT: usize = 1 << 7;
    /// Execute disable bit (`63`):
    /// - If `1` the page is not executable
    /// - If `0` the page is executable
    pub(super) const FLAG_XD: usize = 1 << 63;

    /// A custom flag to mark the entry as taken - meaning that the entry is no free, but isn't
    /// present (ie. not useable yet).
    /// It will become present when the entry is activated, so we can lazily map the page.
    pub(super) const FLAG_TAKEN: usize = 1 << 9;

    /// When lazily mapping a page, we can't pass information regarding the size of the page we
    /// want to map.
    /// So we need to set this flag to mark the entry as a "last entry" in the page table, and now
    /// that we know the last level we can combine that with the `PS` flag to determine the size
    pub(super) const FLAG_LAST_ENTRY: usize = 1 << 10;

    #[inline]
    pub const fn new() -> Self {
        Self {
            data: Self::FLAGS_NONE,
            _arch: PhantomData,
        }
    }

    #[inline]
    pub(super) const fn data(self) -> usize {
        self.data
    }
    
    #[inline]
    const fn get(self, data: usize) -> bool {
        (self.data & data) != 0
    }

    #[inline]
    const fn set(mut self, data: usize, status: bool) -> Self {
        if status {
            self.data |= data;
        } else {
            self.data &= !data;
        }

        self
    }

    #[inline]
    pub(super) const unsafe fn from_raw(data: usize) -> Self {
        Self {
            data,
            _arch: PhantomData,
        }
    }

    #[inline]
    pub(super) const fn set_present(self, status: bool) -> Self {
        self.set(Self::FLAG_P, status)
    }

    #[inline]
    pub const fn set_read_write(self, status: bool) -> Self {
        self.set(Self::FLAG_RW, status)
    }

    #[inline]
    pub const fn set_user_supervisor(self, status: bool) -> Self {
        self.set(Self::FLAG_US, status)
    }

    #[inline]
    pub const fn set_write_through(self, status: bool) -> Self {
        self.set(Self::FLAG_PWT, status)
    }

    #[inline]
    pub const fn set_cache_disable(self, status: bool) -> Self {
        self.set(Self::FLAG_PCD, status)
    }

    #[inline]
    pub(super) const fn set_accessed(self, status: bool) -> Self {
        self.set(Self::FLAG_A, status)
    }

    #[inline]
    pub(super) const fn set_dirty(self, status: bool) -> Self {
        self.set(Self::FLAG_D, status)
    }

    #[inline]
    pub(super) const fn set_page_size(self, status: bool) -> Self {
        self.set(Self::FLAG_PS, status)
    }

    #[inline]
    pub const fn set_global(self, status: bool) -> Self {
        self.set(Self::FLAG_G, status)
    }

    #[inline]
    pub const fn set_hlat(self, status: bool) -> Self {
        self.set(Self::HLAT, status)
    }

    #[inline]
    pub const fn set_execute_disable(self, status: bool) -> Self {
        self.set(Self::FLAG_XD, status)
    }

    #[inline]
    pub(super) const fn set_taken(self, status: bool) -> Self {
        self.set(Self::FLAG_TAKEN, status)
    }

    #[inline]
    pub(super) const fn set_last_entry(self, status: bool) -> Self {
        self.set(Self::FLAG_LAST_ENTRY, status)
    }

    #[inline]
    pub const fn get_present(self) -> bool {
        self.get(Self::FLAG_P)
    }

    #[inline]
    pub const fn get_read_write(self) -> bool {
        self.get(Self::FLAG_RW)
    }

    #[inline]
    pub const fn get_user_supervisor(self) -> bool {
        self.get(Self::FLAG_US)
    }

    #[inline]
    pub const fn get_write_through(self) -> bool {
        self.get(Self::FLAG_PWT)
    }

    #[inline]
    pub const fn get_cache_disable(self) -> bool {
        self.get(Self::FLAG_PCD)
    }

    #[inline]
    pub const fn get_accessed(self) -> bool {
        self.get(Self::FLAG_A)
    }

    #[inline]
    pub const fn get_dirty(self) -> bool {
        self.get(Self::FLAG_D)
    }

    #[inline]
    pub const fn get_page_size(self) -> bool {
        self.get(Self::FLAG_PS)
    }

    #[inline]
    pub const fn get_global(self) -> bool {
        self.get(Self::FLAG_G)
    }

    #[inline]
    pub const fn get_hlat(self) -> bool {
        self.get(Self::HLAT)
    }

    #[inline]
    pub const fn get_execute_disable(self) -> bool {
        self.get(Self::FLAG_XD)
    }

    #[inline]
    pub const fn get_taken(self) -> bool {
        self.get(Self::FLAG_TAKEN)
    }

    #[inline]
    pub const fn get_last_entry(self) -> bool {
        self.get(Self::FLAG_LAST_ENTRY)
    }
}
