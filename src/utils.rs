pub unsafe fn memset(ptr: *mut u8, value: u8, len: usize) {
    unsafe {
        for i in 0..len {
            core::ptr::write_volatile(ptr.add(i), value);
        }
    };
}
