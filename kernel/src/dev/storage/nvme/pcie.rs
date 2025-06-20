use crate::{arch::x86_64::paging::Entry, mem::{mmio::MmioArea, vmm::allocate_pages, VirtAddr}, pcie::PcieDevice};

use modular_bitfield::prelude::*;

// TODO: 
// 1. Calculate number of queues to create, ratio of IO submission to completion queues,
// 2. Maybe use SGLs instead of PRPs
//
// perhaps after implementing network stack, support message model as well

#[repr(C)]
#[derive(Debug)]
struct Queue {
    address: VirtAddr,
    size: u64,
}

#[repr(C)]
#[derive(Debug)]
struct SubmissionQueueEntry {
    command: CommandDword0,
    nsid: u32,
    _reserved0: u32,
    _reserved1: u32,
    metadata_ptr: u64,
    // data pointer
    comamnd_specific: [u32; 6],
}

#[bitfield(bits = 128)]
#[derive(Debug)]
struct CompletionQueueEntry {
    command_specific: B32,
    reserved0: B32,
    submission_queue_head_ptr: B16,
    submission_queue_id: B16,
    comamnd_id: B16,
    phase: B1,
    status: B15,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy)]
struct CommandDword0 {
    opcode: B8,
    operation: B2,
    reserved: B4,
    selection: B2,
    command_id: B16,
}

#[repr(transparent)]
struct Prp(u64);

struct Controller {
    mmio_area: MmioArea<usize, usize, usize>,
}

enum QueueType {
    Submission,
    Completion,
}

impl Controller {
    fn new(pcie_device: PcieDevice) -> Self {

    }

    fn send_command(&self) {

    }

    fn create_io_queue(&self) {

    }

    fn create_admin_queue(&self, size: u64, queue_type: QueueType) -> Queue {
        // XXX: set the correct memory type here
        let queue = Queue {
            address: allocate_pages(1, Entry::FLAG_RW),
            size,
        };

        match queue_type {
            QueueType::Submission => unsafe {self.mmio_area.write(0x28, queue.address.0)},
            QueueType::Completion => unsafe {self.mmio_area.write(0x30, queue.address.0)},
        };

        queue
    }

    fn read() {

    }

    fn write() {

    }
}
