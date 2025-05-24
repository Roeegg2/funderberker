use crate::{
    mem::slab::{SlabAllocatable, SlabAllocator},
    sync::spinlock::{SpinLock, SpinLockDropable},
};
use alloc::boxed::Box;
use core::marker::PhantomData;
use sched::{Schedulable, Scheduler, constant::Constant};
use svm::Svm;
use utils::collections::id::{Id, hander::IdHander};

mod sched;
mod svm;

static SCHEDULER: SpinLock<Constant<Vessel<Svm>>> = SpinLock::new(Constant::new_const());

static VID_ALLOCATOR: SpinLock<IdHander> = SpinLock::new(IdHander::new());

trait VirtTech {
    type VesselControlBlock: Vesselable + 'static;

    fn start();

    fn stop();
}

trait Vesselable: SlabAllocatable + Sized {
    fn new() -> Box<Self, &'static SlabAllocator<Self>>;

    fn load(&mut self);
}

// TODO: Implement the type specific slab allocator, and then use a Box with that custom allocator instead
// of reference to VMCB
/// Represents a general guest execution context
struct Vessel<T>
where
    T: VirtTech,
{
    id: Id,
    phantom: PhantomData<T>,
    control: Box<T::VesselControlBlock, &'static SlabAllocator<T::VesselControlBlock>>,
}

impl<T> Vessel<T>
where
    T: VirtTech,
{
    fn new() -> Self {
        Self {
            id: VID_ALLOCATOR.lock().handout(),
            phantom: PhantomData,
            control: T::VesselControlBlock::new(),
        }
    }

    fn load(&mut self) {
        self.control.load();
    }
}

pub fn start() -> ! {
    Svm::start();
    let vessel: Box<Vessel<Svm>> = Box::new(Vessel::new());
    let mut scheduler = SCHEDULER.lock();
    scheduler.add(vessel);

    scheduler.operation_loop()
}

impl<T> Schedulable for Vessel<T>
where
    T: VirtTech,
{
    fn id(&self) -> Id {
        self.id
    }
}

impl SpinLockDropable for IdHander {}
