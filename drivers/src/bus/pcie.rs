//! Support for the PCI Express bus.

use arch::{
    map_page,
    paging::{Flags, PageSize},
    unmap_page,
    x86_64::paging::pat::PatType,
};
// use crate::acpi::mcfg::ConfigSpace;
use alloc::vec::Vec;
use logger::*;
use utils::{
    mem::{
        PhysAddr,
        mmio::{MmioArea, Offsetable},
    },
    sync::spinlock::{SpinLock, SpinLockable},
};

pub static PCIE_MANAGER: SpinLock<PcieManager> = SpinLock::new(PcieManager::new());

const VENDOR_ID_INVALID: u16 = 0xFFFF;

/// PCI Configuration Space Register Offsets for Type 0 Header
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StandardHeader {
    /// Device ID and Vendor ID (0x00)
    DeviceVendorId = 0x00,
    /// Status and Command (0x04)
    StatusCommand = 0x04,
    /// Class code, Subclass, Prog IF, and Revision ID (0x08)
    ClassRevision = 0x08,
    /// BIST, Header type, Latency Timer, and Cache Line Size (0x0C)
    BistHeaderLatencyCache = 0x0C,
    /// Base Address Register 0 (0x10)
    Bar0 = 0x10,
    /// Base Address Register 1 (0x14)
    Bar1 = 0x14,
    /// Base Address Register 2 (0x18)
    Bar2 = 0x18,
    /// Base Address Register 3 (0x1C)
    Bar3 = 0x1C,
    /// Base Address Register 4 (0x20)
    Bar4 = 0x20,
    /// Base Address Register 5 (0x24)
    Bar5 = 0x24,
    /// Cardbus CIS Pointer (0x28)
    CardbusPointer = 0x28,
    /// Subsystem ID and Subsystem Vendor ID (0x2C)
    SubsystemId = 0x2C,
    /// Expansion ROM Base Address (0x30)
    ExpansionRomBase = 0x30,
    /// Capabilities Pointer (0x34)
    CapabilitiesPointer = 0x34,
    /// Reserved (0x38)
    Reserved = 0x38,
    /// Max latency, Min Grant, Interrupt PIN, and Interrupt Line (0x3C)
    InterruptInfo = 0x3C,
}

/// PCI Configuration Space Register Offsets for Type 1 Header
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PciToPciHeader {
    /// Device ID and Vendor ID (0x00)
    DeviceVendorId = 0x00,
    /// Status and Command (0x04)
    StatusCommand = 0x04,
    /// Class code, Subclass, Prog IF, and Revision ID (0x08)
    ClassRevision = 0x08,
    /// BIST, Header type, Latency Timer, and Cache Line Size (0x0C)
    BistHeaderLatencyCache = 0x0C,
    /// Base Address Register 0 (0x10)
    Bar0 = 0x10,
    /// Base Address Register 1 (0x14)
    Bar1 = 0x14,
    /// Secondary Latency Timer, Subordinate Bus Number, Secondary Bus Number, Primary Bus Number (0x18)
    BusNumbers = 0x18,
    /// Secondary Status, I/O Limit, I/O Base (0x1C)
    SecondaryStatusIo = 0x1C,
    /// Memory Limit, Memory Base (0x20)
    MemoryBase = 0x20,
    /// Prefetchable Memory Limit, Prefetchable Memory Base (0x24)
    PrefetchableMemoryBase = 0x24,
    /// Prefetchable Base Upper 32 Bits (0x28)
    PrefetchableBaseUpper = 0x28,
    /// Prefetchable Limit Upper 32 Bits (0x2C)
    PrefetchableLimitUpper = 0x2C,
    /// I/O Limit Upper 16 Bits, I/O Base Upper 16 Bits (0x30)
    IoBaseUpper = 0x30,
    /// Capability Pointer (0x34)
    CapabilitiesPointer = 0x34,
    /// Expansion ROM Base Address (0x38)
    ExpansionRomBase = 0x38,
    /// Bridge Control, Interrupt PIN, Interrupt Line (0x3C)
    BridgeControlInterrupt = 0x3C,
}

/// PCI Configuration Space Register Offsets for Type 2 Header
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PciToCardbusHeader {
    /// Device ID and Vendor ID (0x00)
    DeviceVendorId = 0x00,
    /// Status and Command (0x04)
    StatusCommand = 0x04,
    /// Class code, Subclass, Prog IF, and Revision ID (0x08)
    ClassRevision = 0x08,
    /// BIST, Header type, Latency Timer, and Cache Line Size (0x0C)
    BistHeaderLatencyCache = 0x0C,
    /// `CardBus` Socket/ExCa Base Address (0x10)
    CardbusSocketBase = 0x10,
    /// Secondary Status, Offset of Capabilities List (0x14)
    SecondaryStatusCapabilities = 0x14,
    /// `CardBus` Latency Timer, Subordinate Bus Number, `CardBus` Bus Number, PCI Bus Number (0x18)
    BusNumbers = 0x18,
    /// Memory Base Address 0 (0x1C)
    MemoryBase0 = 0x1C,
    /// Memory Limit 0 (0x20)
    MemoryLimit0 = 0x20,
    /// Memory Base Address 1 (0x24)
    MemoryBase1 = 0x24,
    /// Memory Limit 1 (0x28)
    MemoryLimit1 = 0x28,
    /// I/O Base Address 0 (0x2C)
    IoBase0 = 0x2C,
    /// I/O Limit 0 (0x30)
    IoLimit0 = 0x30,
    /// I/O Base Address 1 (0x34)
    IoBase1 = 0x34,
    /// I/O Limit 1 (0x38)
    IoLimit1 = 0x38,
    /// Bridge Control, Interrupt PIN, Interrupt Line (0x3C)
    BridgeControlInterrupt = 0x3C,
    /// Subsystem Vendor ID, Subsystem Device ID (0x40)
    SubsystemId = 0x40,
    /// 16-bit PC Card Legacy Mode Base Address (0x44)
    PcCardLegacyBase = 0x44,
}

/// Configuration space base address allocation structure
#[repr(C, packed)]
#[derive(Debug)]
pub struct ConfigSpace {
    pub base_address: u64,
    pub segment_group_number: u16,
    pub start_bus_number: u8,
    pub end_bus_number: u8,
    _reserved: u32,
}

/// Represents a specific `PCIe` device + function.
///
/// NOTE: This does not represent a `PCIe` device in the sense of a physical device, but rather in
/// the sense of a "device function"
pub struct PcieDevice {
    config_space: MmioArea<usize, usize, u32>,
}

/// A manager for all the `PCIe` devices in the system.
pub struct PcieManager {
    devices: Vec<PcieDevice>,
}

impl PcieManager {
    pub fn init(segment_groups: &[ConfigSpace]) -> Result<(), ()> {
        let mut manager = PCIE_MANAGER.lock();
        manager.brute_force_discover(segment_groups);
        manager.load_device_drivers();

        Ok(())
    }
    /// Create a new `PcieManager`.
    const fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Discover all device functions under the given `bus`, `device`, and `segment_group`.
    fn discover_device_functions(&mut self, bus: u8, device: u8, segment_group: &ConfigSpace) {
        if let Some(config_space) = self.check_device(bus, device, 0, segment_group.base_address) {
            self.devices.push(PcieDevice::new(config_space));

            let header_type = unsafe {
                self.devices
                    .last()
                    .unwrap()
                    .config_space
                    .read(StandardHeader::BistHeaderLatencyCache as usize)
                    >> 16
                    & 0xff
            };

            if header_type & 0x80 != 0 {
                for function in 1..=7 {
                    if let Some(config_space) =
                        self.check_device(bus, device, function, segment_group.base_address)
                    {
                        self.devices.push(PcieDevice::new(config_space));
                    } else {
                        continue;
                    }
                }
            }
        }
    }

    /// Check if the given device associated with the given bus, device, and function is present.
    fn check_device(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        segment_group_base: u64,
    ) -> Option<MmioArea<usize, usize, u32>> {
        let config_space: MmioArea<usize, usize, u32> = {
            let phys_addr = PcieDevice::get_base_address(bus, device, function, segment_group_base);
            let virt_addr = unsafe {
                map_page(
                    phys_addr,
                    Flags::new()
                        .set_read_write(true)
                        .set_pat(PatType::WriteThrough, PageSize::size_4kb()),
                    PageSize::size_4kb(),
                )
                .unwrap()
            };

            MmioArea::new(virt_addr.into())
        };

        // Check if the device is present
        let vendor_id =
            unsafe { config_space.read(StandardHeader::DeviceVendorId as usize) & 0xffff };
        if vendor_id == VENDOR_ID_INVALID as u32 {
            // XXX: Set the flags to the correct ones
            unsafe { unmap_page(config_space.base().into(), PageSize::size_4kb()) };
            return None;
        }

        Some(config_space)
    }

    /// Method 1 of discovering PCIe devices: brute force scan the entire `PCIe` space for each segment group
    pub fn brute_force_discover(&mut self, segment_groups: &[ConfigSpace]) {
        for segment_group in segment_groups.iter() {
            for bus in segment_group.start_bus_number..=segment_group.end_bus_number {
                for device in 0..=31 {
                    self.discover_device_functions(bus, device, segment_group);
                }
            }
        }
    }

    pub fn load_device_drivers(&self) {
        for device in self.devices.iter() {
            let (class_code, subclass, prog_if) = unsafe {
                (
                    device
                        .config_space
                        .read(StandardHeader::ClassRevision as usize)
                        >> 24,
                    (device
                        .config_space
                        .read(StandardHeader::ClassRevision as usize)
                        >> 16)
                        & 0xff,
                    (device
                        .config_space
                        .read(StandardHeader::ClassRevision as usize)
                        >> 8)
                        & 0xff,
                )
            };

            match (class_code, subclass, prog_if) {
                (0x1, 0x8, 0x2) => {
                    log_info!("Found NVMe");
                }
                _ => (),
            }
        }
    }
}

impl PcieDevice {
    fn new(config_space: MmioArea<usize, usize, u32>) -> Self {
        Self { config_space }
    }

    #[inline]
    const fn get_base_address(
        bus: u8,
        device: u8,
        function: u8,
        segment_group_base: u64,
    ) -> PhysAddr {
        PhysAddr(
            segment_group_base as usize
                + (((bus as usize) << 20)
                    | ((device as usize) << 15)
                    | ((function as usize) << 12)),
        )
    }
}

impl Drop for PcieDevice {
    fn drop(&mut self) {
        unsafe {
            unmap_page(self.config_space.base().into(), PageSize::size_4kb())
                .expect("Failed to unmap PCIe device config space");
        };
    }
}

impl Offsetable for StandardHeader {
    fn offset(self) -> usize {
        self as usize
    }
}

impl Offsetable for PciToPciHeader {
    fn offset(self) -> usize {
        self as usize
    }
}

impl Offsetable for PciToCardbusHeader {
    fn offset(self) -> usize {
        self as usize
    }
}

impl SpinLockable for PcieManager {}

// XXX: This might not actually be safe
unsafe impl Sync for PcieDevice {}
