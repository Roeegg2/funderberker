//! Mem related usefull wrappers and utility functions

/// Wrapper to memset some region of memory to some value
pub unsafe fn memset(ptr: *mut u8, value: u8, len: usize) {
    unsafe {
        for i in 0..len {
            core::ptr::write_volatile(ptr.add(i), value);
        }
    };
}
