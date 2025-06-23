use super::{Vesselable, VirtTech};

use arch::{
    BASIC_PAGE_SIZE, allocate_pages,
    paging::{Flags, PageSize},
    translate,
    x86_64::{
        X86_64,
        cpu::{
            AmdDr6, AmdDr7, Cr0, Cr2, Cr3, Cr4, Register, Rflags,
            msr::{AmdMsr, Efer, MsrData, rdmsr, wrmsr},
            read_rsp,
        },
        gdt::{Cs, Ds, Es, FullSegmentSelector, Gdt, Ss},
        interrupts::Idt,
    },
};
use logger::*;
use slab::{SlabAllocatable, SlabAllocator};
use utils::{mem::VirtAddr, sync::spinlock::SpinLock};

use alloc::boxed::Box;
use core::{
    arch::x86_64::__cpuid,
    mem::transmute,
    ops::{Deref, DerefMut},
    ptr, todo,
};
use modular_bitfield::prelude::*;
use utils::{
    collections::id::{Id, tracker::IdTracker},
    mem::memset,
    sanity_assert,
};

mod cpu;

// TODO: Make sure the pages are writeback WB and not writethough WT
// TODO: Make this a box to a dyn or something since we might use VMX or something isntead
static VMCB_ALLOCATOR: SlabAllocator<Vmcb> = SlabAllocator::new();

/// The ASID allocator for the guests.
static ASID_ALLOCATOR: SpinLock<IdTracker> = SpinLock::new(IdTracker::uninit());

/// A ZST to implement the `VirtTech` trait on
pub struct Svm;

#[allow(dead_code)]
#[repr(C, packed)]
#[bitfield]
struct Intercepts {
    cr_reads: B16,
    cr_writes: B16,
    dr_reads: B16,
    dr_writes: B16,

    exceptions: B32,

    intr: B1,
    nmi: B1,
    smi: B1,
    init: B1,
    virtual_intr: B1,
    cr0_sel_write: B1,
    idtr_read: B1,
    gdtr_read: B1,
    ldtr_read: B1,
    tr_read: B1,
    idtr_write: B1,
    gdtr_write: B1,
    ldtr_write: B1,
    tr_write: B1,
    rdtsc: B1,
    rdpmc: B1,
    pushf: B1,
    popf: B1,
    cpuid: B1,
    rsm: B1,
    iret: B1,
    intn: B1,
    invd: B1,
    pause: B1,
    hlt: B1,
    invlpg: B1,
    invlpga: B1,
    ioio_prot: B1,
    msr_prot: B1,
    task_switch: B1,
    f_err_freeze: B1,
    shutdown: B1,
    vmrun: B1,
    vmmcall: B1,
    vmload: B1,
    vmsave: B1,
    stgi: B1,
    clgi: B1,
    skinit: B1,
    rdtscp: B1,
    icebp: B1,
    wbinvd: B1,
    monitor: B1,
    mwait: B1,
    mwait_conditional: B1,
    xsetbv: B1,
    rdpru: B1,
    efer_write: B1,
    cr_writes_foo: B16,
    invlpgb_all: B1,
    invlpgb_illegal: B1,
    invpcid: B1,
    mcommit: B1,
    /// NOTE: Check presence of this bit before using it
    tlbsync: B1,
    bus_lock: B1,
    idle_hlt: B1,
    #[skip]
    reserved: B25,
}

#[bitfield]
#[repr(u64)]
struct ExitIntInfo {
    /// The vector of the interrupt or exception
    vector: B8,
    /// The type of the interrupt or exception
    typ: B3,
    /// Indicates whether the guest pushed an error code
    error_code_valid: B1,
    #[skip]
    reserved: B19,
    /// Indicates whether an exit mid interrupt delivery actually happened
    valid: B1,
    /// The error code of the interrupt or exception
    error_code: B32,
}

// TODO: Change the name fo this
#[bitfield]
#[repr(u64)]
struct SvmFlags {
    np_enable: B1,
    sev_enable: B1,
    essev_enable: B1,
    guest_mode_execute_trap: B1,
    sss_check_fn: B1,
    vte_enable: B1,
    ro_guest_page_tables_enable: B1,
    invlpgb_tlbsync_enable: B1,
    reserved_mbz_0: B56,
}

/// The `Control` part of the VMCB
#[allow(dead_code)]
#[repr(C, packed)]
struct ControlArea {
    intercepts: Intercepts,
    reserved_1: [u8; 0x3c - 0x18],
    pause_filter_thershold: u16,
    pause_filter_count: u16,
    iopm_base_pa: u64,
    msrpm_base_pa: u64,
    tsc_offset: u64,
    guest_asid: u32,
    tlb_control: u32,
    vintr: u64,
    partially_reserved_7: u64,
    exitcode: InterceptCode,
    exitinfo1: u64,
    exitinfo2: u64,
    exitintinfo: ExitIntInfo,
    flags: SvmFlags,
    avic_apic_bar_reserved: u64,
    guest_phys_addr_ghcb: u64,
    event_injection: u64,
    n_cr3: u64,
    lbr_virtualization_enable: u64,
    vmcb_clean_bits: u32,
    reserved_11: u32,
    nrip: u64,
    number_of_bytes_fetched: u8,
    guest_instruction_bytes: [u8; 15],
    avic_apic_backing_page_ptr: u64,
    reserved_13: u64,
    avic_logical_table_ptr: u64,
    avic_physical_table_ptr: u64,
    reserved_14: u64,
    vmcb_state_save_ptr: u64,
    reserved_26: [u8; 752],
}

/// The `StateSave` part of the VMCB
#[allow(dead_code)]
#[repr(C, packed)]
struct StateSaveArea {
    es: FullSegmentSelector,
    cs: FullSegmentSelector,
    ss: FullSegmentSelector,
    ds: FullSegmentSelector,
    fs: FullSegmentSelector,
    gs: FullSegmentSelector,
    gdtr: FullSegmentSelector,
    ldtr: FullSegmentSelector,
    idtr: FullSegmentSelector,
    tr: FullSegmentSelector,
    reserved_1: [u8; 0xcb - 0xa0],
    cpl: u8,
    reserved_2: u32,
    efer: u64,
    reserved_3: [u8; 0xe0 - 0xd8],
    perf_ctl0: u64,
    perf_ctr0: u64,
    perf_ctl1: u64,
    perf_ctr1: u64,
    perf_ctl2: u64,
    perf_ctr2: u64,
    perf_ctl3: u64,
    perf_ctr3: u64,
    perf_ctl4: u64,
    perf_ctr4: u64,
    perf_ctl5: u64,
    perf_ctr5: u64,
    /// A field AMD forgot to specify in the docs
    forgot_reserved: u64,
    cr4: Cr4,
    cr3: Cr3,
    cr0: Cr0,
    dr7: AmdDr7,
    dr6: AmdDr6,
    rflags: Rflags,
    rip: usize,
    reserved_4: [u8; 0x1c0 - 0x180],
    instr_retired_ctr: u64,
    perf_ctr_global_sts: u64,
    perf_ctr_global_ctl: u64,
    rsp: u64,
    s_cet: u64,
    ssp: u64,
    isst_addr: u64,
    rax: u64,
    start: u64,
    lstart: u64,
    cstar: u64,
    sf_mask: u64,
    kernel_gs_base: u64,
    sysenter_cs: u64,
    sysenter_esp: u64,
    sysenter_eip: u64,
    cr2: Cr2,
    reserved_6: [u8; 0x268 - 0x248],
    g_pat: u64,
    dbg_ctl: u64,
    br_from: u64,
    br_to: u64,
    last_exception_from: u64,
    last_exception_to: u64,
    dbg_extn_ctl: u64,
    reserved_7: [u8; 64],
    spec_ctrl: u64,
    reserved_8: [u8; 904],
    lbr_stack: [u8; 256],
    lbr_select: u64,
    ibs_fetch_ctl: u64,
    ibs_fetch_linaddr: u64,
    ibs_op_ctl: u64,
    ibs_op_rip: u64,
    ibs_op_data: u64,
    ibs_op_data2: u64,
    ibs_op_data3: u64,
    ibs_dc_linaddr: u64,
    bp_ibstgt_rip: u64,
    ic_ibs_extd_ctl: u64,
    // Rest is reserved until page aligned offset.
    // we don't specifiy it here, since it'll get allocated anyways when we allocate the VMCB page
}

/// The actual VMCB. The definition is broken down into 2 parts so we can force `4096` bytes alignment
#[repr(C, packed)]
pub struct VmcbInner {
    control: ControlArea,
    state_save: StateSaveArea,
}
/// The VMCB structure.
///
/// Each VM has one, so HAV could be used
#[repr(C, align(0x1000))]
pub struct Vmcb(VmcbInner);

/// The possible valid intercept codes that can be found in the `exitcode` field in the VMCB.
///
/// Some intercept codes specify more information in `exitinfo1` and `exitinfo2`
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(i64)]
enum InterceptCode {
    // CR reads
    Cr0Read = 0x0,
    Cr1Read = 0x1,
    Cr2Read = 0x2,
    Cr3Read = 0x3,
    Cr4Read = 0x4,
    Cr5Read = 0x5,
    Cr6Read = 0x6,
    Cr7Read = 0x7,
    Cr8Read = 0x8,
    Cr9Read = 0x9,
    Cr10Read = 0xa,
    Cr11Read = 0xb,
    Cr12Read = 0xc,
    Cr13Read = 0xd,
    Cr14Read = 0xe,
    Cr15Read = 0xf,

    // CR writes
    Cr0Write = 0x0 + 0x10,
    Cr1Write = 0x1 + 0x10,
    Cr2Write = 0x2 + 0x10,
    Cr3Write = 0x3 + 0x10,
    Cr4Write = 0x4 + 0x10,
    Cr5Write = 0x5 + 0x10,
    Cr6Write = 0x6 + 0x10,
    Cr7Write = 0x7 + 0x10,
    Cr8Write = 0x8 + 0x10,
    Cr9Write = 0x9 + 0x10,
    Cr10Write = 0xa + 0x10,
    Cr11Write = 0xb + 0x10,
    Cr12Write = 0xc + 0x10,
    Cr13Write = 0xd + 0x10,
    Cr14Write = 0xe + 0x10,
    Cr15Write = 0xf + 0x10,

    // DR reads
    Dr0Read = 0x0 + 0x20,
    Dr1Read = 0x1 + 0x20,
    Dr2Read = 0x2 + 0x20,
    Dr3Read = 0x3 + 0x20,
    Dr4Read = 0x4 + 0x20,
    Dr5Read = 0x5 + 0x20,
    Dr6Read = 0x6 + 0x20,
    Dr7Read = 0x7 + 0x20,
    Dr8Read = 0x8 + 0x20,
    Dr9Read = 0x9 + 0x20,
    Dr10Read = 0xa + 0x20,
    Dr11Read = 0xb + 0x20,
    Dr12Read = 0xc + 0x20,
    Dr13Read = 0xd + 0x20,
    Dr14Read = 0xe + 0x20,
    Dr15Read = 0xf + 0x20,

    // DR writes
    Dr0Write = 0x0 + 0x30,
    Dr1Write = 0x1 + 0x30,
    Dr2Write = 0x2 + 0x30,
    Dr3Write = 0x3 + 0x30,
    Dr4Write = 0x4 + 0x30,
    Dr5Write = 0x5 + 0x30,
    Dr6Write = 0x6 + 0x30,
    Dr7Write = 0x7 + 0x30,
    Dr8Write = 0x8 + 0x30,
    Dr9Write = 0x9 + 0x30,
    Dr10Write = 0xa + 0x30,
    Dr11Write = 0xb + 0x30,
    Dr12Write = 0xc + 0x30,
    Dr13Write = 0xd + 0x30,
    Dr14Write = 0xe + 0x30,
    Dr15Write = 0xf + 0x30,

    // Exception trigger
    Exception0 = 0x0 + 0x40,
    Exception1 = 0x1 + 0x40,
    Exception2 = 0x2 + 0x40,
    Exception3 = 0x3 + 0x40,
    Exception4 = 0x4 + 0x40,
    Exception5 = 0x5 + 0x40,
    Exception6 = 0x6 + 0x40,
    Exception7 = 0x7 + 0x40,
    Exception8 = 0x8 + 0x40,
    Exception9 = 0x9 + 0x40,
    Exception10 = 0xa + 0x40,
    Exception11 = 0xb + 0x40,
    Exception12 = 0xc + 0x40,
    Exception13 = 0xd + 0x40,
    Exception14 = 0xe + 0x40,
    Exception15 = 0xf + 0x40,
    Exception16 = 0x10 + 0x40,
    Exception17 = 0x11 + 0x40,
    Exception18 = 0x12 + 0x40,
    Exception19 = 0x13 + 0x40,
    Exception20 = 0x14 + 0x40,
    Exception21 = 0x15 + 0x40,
    Exception22 = 0x16 + 0x40,
    Exception23 = 0x17 + 0x40,
    Exception24 = 0x18 + 0x40,
    Exception25 = 0x19 + 0x40,
    Exception26 = 0x1a + 0x40,
    Exception27 = 0x1b + 0x40,
    Exception28 = 0x1c + 0x40,
    Exception29 = 0x1d + 0x40,
    Exception30 = 0x1e + 0x40,
    Exception31 = 0x1f + 0x40,

    Intr = 0x60,
    Nmi = 0x61,
    Smi = 0x62,
    Init = 0x63,
    VirtualIntr = 0x64,
    Cr0SelWrite = 0x65,

    IdtrRead = 0x66,
    GdtrRead = 0x67,
    LdtrRead = 0x68,
    TrRead = 0x69,

    IdtrWrite = 0x6a,
    GdtrWrite = 0x6b,
    LdtrWrite = 0x6c,
    TrWrite = 0x6d,

    Rdtsc = 0x6e,
    Rdpmc = 0x6f,
    Pushf = 0x70,
    Popf = 0x71,
    Cpuid = 0x72,

    Rsm = 0x73,
    Iret = 0x74,
    Swint = 0x75,
    Invd = 0x76,
    Pause = 0x77,
    Hlt = 0x78,
    Invlpg = 0x79,
    Invlpga = 0x7a,
    ExitIoio = 0x7b,
    Msr = 0x7c,
    TaskSwitch = 0x7d,
    FErrFreeze = 0x7e,
    Shutdown = 0x7f,

    Vmrun = 0x80,
    Vmmcall = 0x81,
    Vmload = 0x82,
    Vmsave = 0x83,

    Stgi = 0x84,
    Clgi = 0x85,
    Skinit = 0x86,
    Rdtscp = 0x87,
    Icebp = 0x88,
    Wbinvd = 0x89,
    Monitor = 0x8a,
    Mwait = 0x8b,
    MwaitConditional = 0x8c,
    Rdpru = 0x8e,
    Xsetbv = 0x8d,
    EferWriteTrap = 0x8f,

    Cr0WriteTrap = 0x0 + 0x90,
    Cr1WriteTrap = 0x1 + 0x90,
    Cr2WriteTrap = 0x2 + 0x90,
    Cr3WriteTrap = 0x3 + 0x90,
    Cr4WriteTrap = 0x4 + 0x90,
    Cr5WriteTrap = 0x5 + 0x90,
    Cr6WriteTrap = 0x6 + 0x90,
    Cr7WriteTrap = 0x7 + 0x90,
    Cr8WriteTrap = 0x8 + 0x90,
    Cr9WriteTrap = 0x9 + 0x90,
    Cr10WriteTrap = 0xa + 0x90,
    Cr11WriteTrap = 0xb + 0x90,
    Cr12WriteTrap = 0xc + 0x90,
    Cr13WriteTrap = 0xd + 0x90,
    Cr14WriteTrap = 0xe + 0x90,
    Cr15WriteTrap = 0xf + 0x90,

    Invlpgb = 0xa0,
    InvlpgbIllegal = 0xa1,
    Invpcid = 0xa2,
    Mcommit = 0xa3,
    Tlbsync = 0xa4,
    BusLock = 0xa5,
    IdleHlt = 0xa6,

    Npf = 0x400,
    AvicIncompleteIpi = 0x401,
    AvicNoAccel = 0x402,
    Vmgexit = 0x403,

    Unused = 0xf000_000,

    Invalid = -1,
    Busy = -2,
    IdleRequired = -3,
    InvalidPmc = -4,

    /// The error code QEMU returns instead of `Invalid`. This is a bug in QEMU.
    QemuInvalid = 0xffff_ffff,
}

impl Intercepts {
    /// Intercept all the exception types.
    const ALL_EXCEPTIONS: u32 = 0xffff_ffff;
}

impl Svm {
    /// Allocates and initializes the host state area.
    ///
    /// This area is used by the processor to save the hypervisor context before entering the VM one (i.e.
    /// before VMRUN).
    /// Each core should have one of these of it's own.
    fn init_host_state() {
        // NOTE: Because we've allocated the page manually, it's also up to us to free it in addition
        // to unmapping it!
        //
        // NOTE: IIRC this should be called on each processor!

        // Getting rid of stale data
        let host_state_page =
            allocate_pages(1, Flags::new().set_read_write(true), PageSize::size_4kb())
                .expect("Failed to allocate host state page");

        unsafe {
            // Map the physical page so we can write to it
            let host_state_ptr: *mut u32 = host_state_page.into();

            memset(host_state_ptr.cast::<u8>(), 0x0, BASIC_PAGE_SIZE);
        };

        unsafe {
            let phys_addr = translate::<X86_64>(host_state_page).unwrap();
            // Breaking the physical address of the page into parts, so we can write it to the MSR
            let low = (phys_addr.0 & 0xffff_ffff) as u32;
            let high = ((phys_addr.0 >> 32) & 0xffff_ffff) as u32;

            // Set the host state save area
            wrmsr(AmdMsr::VmHsavePa, MsrData { low, high });
        };
    }

    /// Enables the option to enter SVM operation.
    fn enable() {
        Self::check_support();
        Self::check_firmware_disabled();

        unsafe {
            let mut data: u64 = rdmsr(AmdMsr::Efer).into();
            data |= Efer::SVM; // Set the SVM enable bit in EFER
            wrmsr(AmdMsr::Efer, data.into());
        }

        log_info!("Enabled SVM sucessfully");
    }

    /// Make sure SVM is supported on this CPU
    fn check_support() {
        const SVM_SUPPORT_ECX_BIT: u32 = 1 << 2;

        unsafe {
            assert!(
                __cpuid(0x8000_0001).ecx & SVM_SUPPORT_ECX_BIT != 0,
                "SVM isn't supported on this processor"
            );
        };
    }

    /// Perform a check to see if virtualization is disabled by the firmware.
    fn check_firmware_disabled() {
        const SVM_DISABLE: u32 = 1 << 4;
        const CPUID_SVM_BRANCH: u32 = 0x8000_000a;
        const SVML: u32 = 1 << 2;

        let vmcr = unsafe { rdmsr(AmdMsr::VmCr) };

        if vmcr.low & SVM_DISABLE != 0 {
            unsafe {
                assert!(
                    __cpuid(CPUID_SVM_BRANCH).edx & SVML != 0,
                    "SVM is disabled by firmware. Change your BIOS/UEFI settings to enable it."
                );
            };

            panic!(
                "SVM is disabled by firmware but unlockable with key. Sadly Funderberker doesn't support this yet"
            );
        }
    }

    /// Initializes the ASID allocator
    fn init_asid_allocator() {
        let mut allocator = ASID_ALLOCATOR.lock();

        let start_id = Id(1); // ASID 0 is reserved for the host, so we start from 1
        // NOTE: Adding 1 here since the max ASID is inclusive, so we need to add 1 to the end of
        // the range
        let end_id = unsafe { Id(__cpuid(0x8000_000a).ebx as usize + 1) };

        *allocator = IdTracker::new(start_id..end_id)
    }
}

impl Vmcb {
    /// Creates a new VMCB.
    ///
    /// The specs obligates us to just have it all zeroed out, and then initialize whatever fields we
    /// wish.
    #[inline]
    const fn uninit() -> Self {
        unsafe { core::mem::zeroed() }
    }

    /// Sanity checking the guest state of the VMCB after it has been initialized.
    #[inline]
    fn sanity_check_guest_state(&self) {
        sanity_assert!(
            !(self.state_save.efer & Efer::SVM == 0),
            "SVM is not enabled in EFER"
        );
        sanity_assert!(
            !(self.state_save.cr0.cd() == 0 && self.state_save.cr0.nw() != 0),
            "CR0.CD is set but CR0.NW is not"
        );
        sanity_assert!(
            !(self.state_save.cr0.reserved_mbz_3() != 0),
            "CR0 has reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.cr3.reserved_mbz_0() != 0
                || self.state_save.cr3.reserved_mbz_1() != 0),
            "CR3 has reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.cr4.reserved_mbz_0() != 0
                || self.state_save.cr4.reserved_mbz_1() != 0
                || self.state_save.cr4.reserved_mbz_2() != 0),
            "CR4 has reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.dr6.reserved_mbz_1() != 0),
            "DR6 has last reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.dr7.reserved_mbz_1() != 0),
            "DR6 has last reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.efer & !Efer::ALL != 0),
            "EFER has reserved bits set"
        );
        sanity_assert!(
            !(self.state_save.efer & Efer::LMA != 0
                && self.state_save.cr0.pg() != 0
                && self.state_save.cr4.pae() == 0),
            "EFER.LMA is set but CR0.PG is set and CR4.PAE is not set"
        );
        sanity_assert!(
            !(self.state_save.efer & Efer::LMA != 0
                && self.state_save.cr0.pg() != 0
                && self.state_save.cr0.pe() == 0),
            "EFER.LMA is set but CR0.PG is set and CR4.PAE is not set"
        );
        // TODO: EFER.LME, CR0.PG, CR4.PAE, CS.L, and CS.D are all non-zero.
        // sanity_assert!(!(self.state_save.efer & Efer::LME == 0 && self.state_save.cr0.pg() != 0 && self.state_save.cr4.pae() != 0 &&
        //     self.state_save.cs.limit != 0 && self.state_save.cs.selector. != 0),
        //     "EFER.LME is not set but CR0.PG, CR4.PAE, CS.L, and CS.D are all non-zero");
        sanity_assert!(
            !(self.control.intercepts.vmrun() == 0),
            "VMRUN intercept is not set in the control area"
        );
        // TODO: check msr and ioio
        // TODO: illegal event injection
        sanity_assert!(!(self.control.guest_asid == 0), "Guest ASID is zero");
        // TODO: S_CET reserved bits
        sanity_assert!(
            !(self.state_save.cr4.cet() != 0 && self.state_save.cr0.wp() == 0),
            "CR4.CET is set but CR0.WP is not set"
        );
        // TODO: U_SET and combination checking
        // TODO: U_SET reserved bits

        // TODO: Processor should always support long mode
    }

    /// Makes sure the processor supports nested paging before we try to set it up.
    #[inline]
    fn check_nested_paging_support() {
        const NESTED_PAGING_BIT: u32 = 1 << 0;

        unsafe {
            assert!(
                __cpuid(0x8000_000a).edx & NESTED_PAGING_BIT != 0,
                "Nested paging is not supported on this processor"
            );
        }
    }

    unsafe fn setup_nested_paging(&mut self) {
        // Make sure nested paging is supported before we try to set it up
        Self::check_nested_paging_support();

        // Enable the feature in the VMCB
        self.control.flags.set_np_enable(1);

        // let (n_cr3_addr, n_cr3) = create_guest_address_space(4);
        // self.control.n_cr3 = n_cr3_addr.0 as u64;
        // self.state_save.cr3 = (new_guest_page_table(n_cr3).0.0 as u64).into();
        // self.cr3 = new_page_table();
        // self.n_cr3 = new_mem_space();

        // TODO: dop the sanity cehcks specified on the nested paging section

        // setup gcr3
        // setup ncr3
        // need to sanity check nCR3 mbz bits aren't set and G_PAT.PA don't have unsupported type
        // encdiong AND G_PAT reserved bits are zero
        //
        //
        // NOTE: check support for nested paging before setiting it up. this is done using CPUID Fn8000_000A_EDX[NP] = 1
        // VMRUN is executed with hCR0.PG cleared to zero and NP_ENABLE set to 1, VMRUN terminates with #VMEXIT(VMEXIT_INVALID
    }

    /// Initializes the guest state of the VMCB.
    ///
    /// The processor will load these fields when `VMRUN` is executed.
    ///
    /// NOTE: Not every combination of fields is valid. See the AMD APM Vol 2, `Canonicalization
    /// and Consistency Checks`
    fn init_guest_state(&mut self, rip: usize) {
        let gdt = {
            let virt_addr: VirtAddr = Gdt::read_gdtr().into();
            let ptr: *mut Gdt = virt_addr.into();
            unsafe { ptr.as_mut().unwrap() }
        };

        unsafe {
            // let phys_page = pmm::get()
            //     .allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap())
            //     .unwrap();
            // let virt_addr = VirtAddr(0x40_000);
            //
            // map_page_to(phys_page, virt_addr, Entry::FLAG_RW);
            //
            // memcpy(virt_addr.into(), GUEST_CODE.as_ptr(), GUEST_CODE.len());

            self.state_save.cs = gdt.read_full_selector(Cs::read().0);
            self.state_save.rip = rip;
            self.state_save.rflags = transmute(1_u64 << 1);
            self.state_save.rax = 0; // TODO: Not sure about RAX
            self.state_save.ss = gdt.read_full_selector(Ss::read().0);
            self.state_save.rsp = read_rsp() as u64;
            self.state_save.cr0 = Cr0::new().with_cd(1).with_nw(1);
            self.state_save.cr3 = Cr3::read();
            self.state_save.cr4 = Cr4::read();
            self.state_save.efer = rdmsr(AmdMsr::Efer).into();
            self.state_save.idtr = Idt::read_idtr().into();
            self.state_save.gdtr = Gdt::read_gdtr().into();
            self.state_save.es = gdt.read_full_selector(Es::read().0);
            self.state_save.ds = gdt.read_full_selector(Ds::read().0);
            self.state_save.dr6 = AmdDr6::read();
            self.state_save.dr7 = AmdDr7::read();
            self.state_save.cpl = 0; // We start in ring 0

            self.control.intercepts.set_vmrun(1);
            self.control.intercepts.set_cpuid(1);
            self.control.intercepts.set_hlt(1);
            self.control
                .intercepts
                .set_exceptions(Intercepts::ALL_EXCEPTIONS);
            self.control.guest_asid = ASID_ALLOCATOR.lock().allocate().unwrap().0 as u32;

            // self.setup_nested_paging();
        }

        self.sanity_check_guest_state();
    }

    /// Handles the intercept if the VM was in the middle of an interrupt delivery
    fn handle_intercept_during_int(&mut self) {
        if self.control.exitintinfo.valid() == 0 {
            // No interrupt delivery in progress, nothing to do
            return;
        }

        todo!(
            "Recieved interrupt during VMEXIT, but not implemented yet {:x}",
            self.control.exitintinfo.vector()
        );
    }

    /// Handles the VMEXIT when testing the intercepts.
    #[cfg(test)]
    fn test_intercepts_handle_vmexit(&mut self, expceted_exit_code: InterceptCode) {
        let exit_code = self.control.exitcode;

        self.handle_intercept_during_int();
        assert_eq!(
            exit_code, expceted_exit_code,
            "Expected exit code {:?}, got {:?}",
            expceted_exit_code, exit_code
        );
    }

    // TODO: get rid of this function
    /// Same as run, but with test VMEXIT handling
    #[cfg(test)]
    fn run_test(&mut self, expected_exit_code: InterceptCode) {
        let ptr = ptr::from_mut(self);
        let phys_addr = translate::<X86_64>(ptr.into()).unwrap();

        unsafe {
            cpu::vmrun(phys_addr);
        };

        self.test_intercepts_handle_vmexit(expected_exit_code);
    }

    /// Handles the VMEXIT.
    ///
    /// This if the very first function that is called when a `VMEXIT` happens.
    fn handle_vmexit(&mut self) {
        // hardware does these things on vmexit:
        // 1. clears GIF so the switch isn't interrupted
        // 2. writes to VMCB the current state + exitcode info
        // 3. clears intercepts
        // 4. sets guest ASID to 0
        // 5. clears v_irq, v_intr_masking and tsc offset
        // 6. reloads processor state with the saved host state from before VMRUN
        // ... and oither things

        // TODO: Check Fn8000_000A_EDX[NRIPS] for nrip support
        // TODO: Check exitintinfo to see if was in the middle of int/exception handling

        let exit_code = self.control.exitcode;
        log_info!("VMEXIT with exitcode: {:?}", exit_code);

        self.handle_intercept_during_int();

        match exit_code {
            InterceptCode::Cpuid => {
                println!("CPUID intercept triggered");
            }
            InterceptCode::Hlt => {
                println!("HLT intercept triggered");
            }
            InterceptCode::Vmrun | InterceptCode::Vmload | InterceptCode::Vmsave => {
                log_err!("Nested virtualization is not supported yet.");
            }
            InterceptCode::Invalid | InterceptCode::QemuInvalid => {
                panic!("Unknown invalid VMCB state. Fatal error");
            }
            _ => panic!("Unhandled VMEXIT"),
        }
    }
}

impl VirtTech for Svm {
    type VesselControlBlock = Vmcb;

    fn start() {
        Self::enable();
        Self::init_asid_allocator();
        Self::init_host_state();

        log_info!("Started SVM operation successfully");
    }
}

impl Vesselable for Vmcb {
    fn new(rip: usize) -> Box<Self, &'static SlabAllocator<Self>> {
        let mut vmcb = Box::new_in(Self::uninit(), &VMCB_ALLOCATOR);

        vmcb.init_guest_state(rip);

        vmcb
    }

    fn run(&mut self) {
        let ptr = ptr::from_mut(self);
        let phys_addr = translate::<X86_64>(ptr.into()).unwrap();

        unsafe {
            cpu::vmrun(phys_addr);
        };

        self.handle_vmexit();
    }
}

impl Deref for Vmcb {
    type Target = VmcbInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Vmcb {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl SlabAllocatable for Vmcb {}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use core::mem::{offset_of, size_of};
//     use macros::test_fn;
//
//     #[test_fn]
//     fn test_cpuid_intercept() {
//         fn to_run() {
//             // This function is called by the guest code to trigger a CPUID intercept
//             unsafe {
//                 asm!("cpuid");
//             }
//         }
//
//         let mut vmcb = Vmcb::new(
//             translate(VirtAddr(to_run as usize))
//                 .unwrap()
//                 .0,
//         );
//         vmcb.run_test(InterceptCode::Cpuid);
//     }
//
//     // TODO: Make this a compile time check
//     #[test_fn]
//     fn test_vmcb_layout() {
//         assert_eq!(offset_of!(VmcbInner, control), 0);
//         assert_eq!(offset_of!(VmcbInner, state_save), size_of::<ControlArea>());
//
//         // ControlArea offset checks
//         assert_eq!(offset_of!(ControlArea, pause_filter_thershold), 0x03c);
//         assert_eq!(offset_of!(ControlArea, vintr), 0x060);
//         assert_eq!(offset_of!(ControlArea, event_injection), 0x0a8);
//
//         // StateSaveArea offset checks (relative to StateSaveArea start)
//         assert_eq!(offset_of!(StateSaveArea, es), 0x000);
//         assert_eq!(offset_of!(StateSaveArea, cpl), 0xcb);
//         assert_eq!(offset_of!(StateSaveArea, cr4), 0x148);
//         assert_eq!(offset_of!(StateSaveArea, rax), 0x1f8);
//         assert_eq!(offset_of!(StateSaveArea, g_pat), 0x268);
//         assert_eq!(offset_of!(StateSaveArea, dbg_extn_ctl), 0x298);
//         assert_eq!(offset_of!(StateSaveArea, spec_ctrl), 0x2e0);
//         assert_eq!(offset_of!(StateSaveArea, ic_ibs_extd_ctl), 0x7c0);
//
//         // FullSegmentSelector size and offset checks
//         assert_eq!(size_of::<FullSegmentSelector>(), 16);
//         assert_eq!(size_of::<u16>(), 2); // sel
//         assert_eq!(size_of::<u16>(), 2); // attr
//         assert_eq!(size_of::<u32>(), 4); // limit
//         assert_eq!(size_of::<u64>(), 8); // base
//     }
// }
