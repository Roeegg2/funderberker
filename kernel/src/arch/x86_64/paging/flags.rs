use crate::{
    arch::x86_64::X86_64,
    mem::paging::{Flags, PageSize},
};

use super::pat::{PatEntry, PatType};

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
    ///
    /// Also PAT bit 0
    pub(super) const FLAG_PWT: usize = 1 << 3;
    /// Page-level cache disable bit (`4`):
    /// - If `1` the page is not cacheable
    /// - If `0` the page is cacheable
    ///
    /// Also PAT bit 1
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
    /// PAT bit 3 (on PDPE!)
    pub(super) const FLAG_BIG_PAGES_PAT: usize = 1 << 12;
    /// PAT bit 3 (on PTE!)
    pub(super) const FLAG_4KB_PAT: usize = 1 << 7;
    /// Execute disable bit (`63`):
    /// - If `1` the page is not executable
    /// - If `0` the page is executable
    pub(super) const FLAG_XD: usize = 1 << 63;

    /// Custom flag to mark a page as an allocated one, so we should free it when the time comes
    pub(super) const FLAG_ALLOCATED: usize = 1 << 9;

    /// Set this flag to mark the entry as a "last entry" in the page table, and now
    /// that we know the last level we can combine that with the `PS` flag to determine the size
    pub(super) const FLAG_LAST_ENTRY: usize = 1 << 10;

    /// Create a new, empty `Flags` instance
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        unsafe { Self::from_raw(Self::FLAGS_NONE) }
    }

    /// Set the bits matching the specified `PatType`, when the page is of `PageSize`.
    #[inline]
    #[must_use]
    pub fn set_pat(self, pat_type: PatType, page_size: PageSize<X86_64>) -> Self {
        let pat: PatEntry = pat_type.into();
        if page_size == PageSize::size_4kb() {
            self.set_pat_4kb((pat as usize & 0b100) != 0)
        } else {
            self.set_pat_big_pages((pat as usize & 0b100) != 0)
        }
        .set_pwt((pat as usize & 0b010) != 0)
        .set_pcd((pat as usize & 0b001) != 0)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_present(self, status: bool) -> Self {
        self.set(Self::FLAG_P, status)
    }

    #[inline]
    #[must_use]
    pub const fn set_read_write(self, status: bool) -> Self {
        self.set(Self::FLAG_RW, status)
    }

    #[inline]
    #[must_use]
    pub const fn set_user_supervisor(self, status: bool) -> Self {
        self.set(Self::FLAG_US, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_pwt(self, status: bool) -> Self {
        self.set(Self::FLAG_PWT, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_pcd(self, status: bool) -> Self {
        self.set(Self::FLAG_PCD, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_pat_big_pages(self, status: bool) -> Self {
        self.set(Self::FLAG_BIG_PAGES_PAT, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_pat_4kb(self, status: bool) -> Self {
        self.set(Self::FLAG_4KB_PAT, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_accessed(self, status: bool) -> Self {
        self.set(Self::FLAG_A, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_dirty(self, status: bool) -> Self {
        self.set(Self::FLAG_D, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_page_size(self, status: bool) -> Self {
        self.set(Self::FLAG_PS, status)
    }

    #[inline]
    #[must_use]
    pub const fn set_global(self, status: bool) -> Self {
        self.set(Self::FLAG_G, status)
    }

    #[inline]
    #[must_use]
    pub const fn set_hlat(self, status: bool) -> Self {
        self.set(Self::HLAT, status)
    }

    #[inline]
    #[must_use]
    pub const fn set_execute_disable(self, status: bool) -> Self {
        self.set(Self::FLAG_XD, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_allocated(self, status: bool) -> Self {
        self.set(Self::FLAG_ALLOCATED, status)
    }

    #[inline]
    #[must_use]
    pub(super) const fn set_last_entry(self, status: bool) -> Self {
        self.set(Self::FLAG_LAST_ENTRY, status)
    }

    #[inline]
    #[must_use]
    pub const fn get_present(self) -> bool {
        self.get(Self::FLAG_P)
    }

    #[inline]
    #[must_use]
    pub const fn get_read_write(self) -> bool {
        self.get(Self::FLAG_RW)
    }

    #[inline]
    #[must_use]
    pub const fn get_user_supervisor(self) -> bool {
        self.get(Self::FLAG_US)
    }

    #[inline]
    #[must_use]
    pub const fn get_write_through(self) -> bool {
        self.get(Self::FLAG_PWT)
    }

    #[inline]
    #[must_use]
    pub const fn get_cache_disable(self) -> bool {
        self.get(Self::FLAG_PCD)
    }

    #[inline]
    #[must_use]
    pub const fn get_accessed(self) -> bool {
        self.get(Self::FLAG_A)
    }

    #[inline]
    #[must_use]
    pub const fn get_dirty(self) -> bool {
        self.get(Self::FLAG_D)
    }

    #[inline]
    #[must_use]
    pub const fn get_page_size(self) -> bool {
        self.get(Self::FLAG_PS)
    }

    #[inline]
    #[must_use]
    pub const fn get_global(self) -> bool {
        self.get(Self::FLAG_G)
    }

    #[inline]
    #[must_use]
    pub const fn get_hlat(self) -> bool {
        self.get(Self::HLAT)
    }

    #[inline]
    #[must_use]
    pub const fn get_execute_disable(self) -> bool {
        self.get(Self::FLAG_XD)
    }

    #[inline]
    #[must_use]
    pub const fn get_allocated(self) -> bool {
        self.get(Self::FLAG_ALLOCATED)
    }

    #[inline]
    #[must_use]
    pub const fn get_last_entry(self) -> bool {
        self.get(Self::FLAG_LAST_ENTRY)
    }
}
