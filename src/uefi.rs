use crate::arch::x86_64::serial::SerialPort;
use core::{ffi::c_void, panic::PanicInfo, slice};

pub type Handle = *mut c_void;
pub type Guid = (u32, u16, u16, u8, u8, u8, u8, u8, u8, u8, u8);

const ERROR_BIT: isize = 1 << (usize::BITS - 1);

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
#[repr(C)]
pub enum Status {
    Success = 0,
    /// The string contained characters that could not be rendered and were skipped.
    WarnUnknownGlyph = 1,
    /// The handle was closed, but the file was not deleted.
    WarnDeleteFailure = 2,
    /// The handle was closed, but the data to the file was not flushed properly.
    WarnWriteFailure = 3,
    /// The resulting buffer was too small, and the data was truncated.
    WarnBufferTooSmall = 4,
    /// The data has not been updated within the timeframe set by local policy.
    WarnStaleData = 5,
    /// The resulting buffer contains UEFI-compliant file system.
    WarnFileSystem = 6,
    /// The operation will be processed across a system reset.
    WarnResetRequired = 7,
    /// The image failed to load.
    LoadError = ERROR_BIT | 1,
    /// A parameter was incorrect.
    InvalidParameter = ERROR_BIT | 2,
    /// The operation is not supported.
    Unsupported = ERROR_BIT | 3,
    /// The buffer was not the proper size for the request.
    BadBufferSize = ERROR_BIT | 4,
    /// The buffer is not large enough to hold the requested data.
    /// The required buffer size is returned in the appropriate parameter.
    BufferTooSmall = ERROR_BIT | 5,
    /// There is no data pending upon return.
    NotReady = ERROR_BIT | 6,
    /// The physical device reported an error while attempting the operation.
    DeviceError = ERROR_BIT | 7,
    /// The device cannot be written to.
    WriteProtected = ERROR_BIT | 8,
    /// A resource has run out.
    OutOfResources = ERROR_BIT | 9,
    /// An inconstency was detected on the file system.
    VolumeCorrupted = ERROR_BIT | 10,
    /// There is no more space on the file system.
    VolumeFull = ERROR_BIT | 11,
    /// The device does not contain any medium to perform the operation.
    NoMedia = ERROR_BIT | 12,
    /// The medium in the device has changed since the last access.
    MediaChanged = ERROR_BIT | 13,
    /// The item was not found.
    NotFound = ERROR_BIT | 14,
    /// Access was denied.
    AccessDenied = ERROR_BIT | 15,
    /// The server was not found or did not respond to the request.
    NoResponse = ERROR_BIT | 16,
    /// A mapping to a device does not exist.
    NoMapping = ERROR_BIT | 17,
    /// The timeout time expired.
    Timeout = ERROR_BIT | 18,
    /// The protocol has not been started.
    NotStarted = ERROR_BIT | 19,
    /// The protocol has already been started.
    AlreadyStarted = ERROR_BIT | 20,
    /// The operation was aborted.
    Aborted = ERROR_BIT | 21,
    // There are some more errors, but I don't need them anyway
}

#[derive(Debug)]
#[repr(C)]
pub struct ConfigurationTable {
    pub vendor_guid: Guid,
    pub vendor_table: *mut c_void,
}

#[derive(Debug)]
#[repr(C)]
struct Header {
    signature: u64,
    revision: u32,
    size: u32,
    crc: u32,
    _reserved: u32,
}

#[derive(Debug)]
#[allow(dead_code)]
#[repr(C)]
pub enum MemoryType {
    ReservedMemoryType,
    LoaderCode,
    LoaderData,
    BootServicesCode,
    BootServicesData,
    RuntimeServicesCode,
    RuntimeServicesData,
    ConventionalMemory,
    UnusableMemory,
    ACPIReclaimMemory,
    ACPIMemoryNVS,
    MemoryMappedIO,
    MemoryMappedIOPortSpace,
    PalCode,
    PersistentMemory,
    UnacceptedMemoryType,
    MaxMemoryType,
}

// NOTE: Never use sizeof::<MemoryDescriptor>()!!
#[derive(Debug)]
#[repr(C)]
pub struct MemoryDescriptor {
    pub mem_type: MemoryType,
    pub phys_addr_start: u64,
    pub virt_addr_start: u64,
    pub page_count: u64,
    pub attr: u64,
    pub _reserved: u64, // added this because for some reason UEFI reports MemoryDescriptor size is 48
                        // instead of 40, even though the structure here is OK.
}

#[repr(C)]
pub struct BootServices {
    header: Header,
    raise_tpl: *mut c_void,
    restore_tpl: *mut c_void,
    allocate_pages: *mut c_void,
    free_pages: *mut c_void,
    get_memory_map: unsafe extern "efiapi" fn(
        size: *mut usize,
        map: *mut MemoryDescriptor,
        key: *mut usize,
        desc_size: *mut usize,
        desc_version: *mut u32,
    ) -> Status,
    allocate_pool: unsafe extern "efiapi" fn(
        pool_type: MemoryType,
        size: usize,
        buffer: *mut *mut u8,
    ) -> Status,
    free_pool: *mut c_void,
    create_event: *mut c_void,
    set_timer: *mut c_void,
    wait_for_event: *mut c_void,
    signal_event: *mut c_void,
    close_event: *mut c_void,
    check_event: *mut c_void,
    install_protocol_interface: *mut c_void,
    reinstall_protocol_interface: *mut c_void,
    uninstall_protocol_interface: *mut c_void,
    handle_protocol: *mut c_void,
    reserved: *mut c_void,
    register_protocol_notify: *mut c_void,
    locate_handle: *mut c_void,
    locate_device_path: *mut c_void,
    install_configuration_table: *mut c_void,
    load_image: *mut c_void,
    start_image: *mut c_void,
    exit: *mut c_void,
    unload_image: *mut c_void,
    exit_boot_services: unsafe extern "efiapi" fn(image_handle: Handle, map_key: usize) -> Status,
    get_next_monotonic_count: *mut c_void,
    stall: *mut c_void,
    set_watchdog_timer: *mut c_void,
    connect_controller: *mut c_void,
    disconnect_controller: *mut c_void,
    open_protocol: *mut c_void,
    close_protocol: *mut c_void,
    open_protocol_information: *mut c_void,
    protocols_per_handle: *mut c_void,
    locate_handle_buffer: *mut c_void,
    locate_protocol: *mut c_void,
    install_multiple_protocol_interfaces: *mut c_void,
    uninstall_multiple_protocol_interfaces: *mut c_void,
    calculate_crc32: *mut c_void,
    copy_mem: *mut c_void,
    set_mem: *mut c_void,
    create_event_ex: *mut c_void,
}

#[derive(Debug)]
#[repr(C)]
pub struct SystemTable {
    header: Header,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    stdin_handle: Handle,
    stdin: *mut c_void,
    stdout_handle: Handle,
    stdout: *mut c_void,
    stderr_handle: Handle,
    stderr: *mut c_void,
    runtime_services: *mut c_void,
    boot_services: *mut BootServices,
    num_of_config_tables: usize,
    config_tables: *mut ConfigurationTable,
}

impl ConfigurationTable {
    /// search for the `vendor_guid` in the SystemTable's configuration tables
    pub fn get_vendor_table(
        &mut self,
        num_of_config_tables: usize,
        vendor_guid: &Guid,
    ) -> Option<*const c_void> {
        let config_tables = unsafe { slice::from_raw_parts(self, num_of_config_tables) };

        for table in config_tables {
            if table.vendor_guid == *vendor_guid {
                return Some(table.vendor_table);
            }
        }

        None
    }
}

impl SystemTable {
    pub fn exit_boot_services(&self, handle: Handle) -> (*mut MemoryDescriptor, usize, usize) {
        let mut mem_map_size: usize = 0;
        let mut mem_map: *mut u8 = core::ptr::null_mut();
        let mut key = 0;
        let mut descr_size = 0;
        let mut descr_version = 0;

        // get reference to boot services
        let boot_services = unsafe { self.boot_services.as_ref().unwrap() };

        unsafe {
            // we can pass 'core::ptr::null_mut()' since we don't actually want to get the pointer
            // right now. we just want to get the required size so we can allocate
            assert_eq!(
                (boot_services.get_memory_map)(
                    &mut mem_map_size,
                    core::ptr::null_mut(),
                    &mut key,
                    &mut descr_size,
                    &mut descr_version
                ),
                Status::BufferTooSmall
            );

            // 'MemoryType::LoaderData' Since we want the memory type to belong to us (the loaded
            // image)
            // we allocate 2 additional entries just in case the allocation of the memory map also
            // creates new memory descriptors
            mem_map_size = mem_map_size + (2 * descr_size);
            assert_eq!(
                (boot_services.allocate_pool)(MemoryType::LoaderData, mem_map_size, &mut mem_map),
                Status::Success
            );
        }

        let mem_map = mem_map as *mut MemoryDescriptor;
        unsafe {
            // now call with the intention of actually getting the memory map
            assert_eq!(
                (boot_services.get_memory_map)(
                    &mut mem_map_size,
                    mem_map,
                    &mut key,
                    &mut descr_size,
                    &mut descr_version
                ),
                Status::Success
            );

            // now that we have the key we can safely exit boot services
            assert_eq!(
                (boot_services.exit_boot_services)(handle, key),
                Status::Success
            );
        }

        (mem_map, mem_map_size, descr_size)
    }
}

#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(handle: Handle, system_table: *mut SystemTable) -> ! {
    // initilize COMM1 serial port
    SerialPort::Comm1.init().unwrap();
    log!("initilized serial port COMM1 successfully!");

    // get the system_table as a mut reference
    let system_table = unsafe { system_table.as_mut().unwrap() };
    // exit boot services

    let (mem_map, mem_map_size, mem_descr_size) = system_table.exit_boot_services(handle);

    log!("exited boot services successfully!");
    let config_tables = unsafe { system_table.config_tables.as_mut() }.unwrap();

    // start funderberker!
    crate::funderberker_main(
        mem_map,
        mem_map_size,
        mem_descr_size,
        config_tables,
        system_table.num_of_config_tables,
    );

    loop {}
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}
