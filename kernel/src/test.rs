use crate::mem::{pmm::PmmAllocator, PhysAddr};

pub(super) fn start_testing() {
    test_pmm();
    test_slab();
}

fn test_pmm() {
    let allocator = crate::mem::pmm::get();

    if allocator.is_page_free(PhysAddr(50 * 0x1000)).unwrap() {
        allocator.alloc_at(PhysAddr(50), 1).unwrap();
        unsafe {allocator.free(PhysAddr(50 * 0x1000), 1).unwrap()};
    }

    let a = allocator.alloc_any(1, 4).unwrap();
    let b = allocator.alloc_any(5, 20).unwrap();
    if b.0 % 0x5000 != 0 {
        panic!("Alignment failed");
    }
    unsafe {allocator.free(a, 4).unwrap()};
    let c = allocator.alloc_any(1, 5).unwrap();
    unsafe {allocator.free(b, 20).unwrap()};
    unsafe {allocator.free(c, 5).unwrap()};

}

fn test_slab() {
    let allocator = crate::mem::vmm::
}
