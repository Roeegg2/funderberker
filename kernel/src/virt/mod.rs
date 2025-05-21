use core::marker::PhantomData;

use utils::collections::id::hander::IdHander;

pub mod svm;

trait VirtTech {
    type VesselControlBlock: Vesselable;
    
    fn start();

    fn stop();
}

trait Vesselable {
    fn new() -> &'static mut Self;

    fn load(&mut self);
}

static VID_ALLOCATOR: IdHander = IdHander::new();

/// Represents a guest execution context
struct Vessel<T> where T: VirtTech {
    id: usize,
    phantom: PhantomData<T>,
    // virt_tech: &'a T,
}

impl<T> Vessel<T> where T: VirtTech {
    // fn new() -> Self<T> {
    //
    // }
}
