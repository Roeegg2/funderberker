use super::{Vesselable, VirtTech};
use crate::{
    arch::{
        BASIC_PAGE_SIZE,
        x86_64::{
            cpu::{
                self, AmdMsr, rdmsr, read_cs, read_dr6, read_dr7, read_ds, read_es, read_ss, wrmsr,
            },
            event::__isr_stub_generic_irq_isr,
            gdt::{FullSegmentSelector, Gdt},
            interrupts::Idt,
            paging::Entry,
        },
    },
    mem::{
        PhysAddr, VirtAddr,
        pmm::{self, PmmAllocator},
        slab::{SlabAllocatable, SlabAllocator},
        vmm::{allocate_pages, map_page, translate},
    },
    read_cr,
    sync::spinlock::{SpinLock, SpinLockDropable},
};
use alloc::boxed::Box;
use core::{
    arch::{asm, x86_64::__cpuid},
    cell::{SyncUnsafeCell, UnsafeCell},
    num::NonZero,
    ops::{Deref, DerefMut},
    ptr,
};
use utils::{mem::memset, sanity_assert};

static VMCB_ALLOCATOR: SlabAllocator<Vmcb> = SlabAllocator::new();

/// A ZST to implement the `VirtTech` trait on
pub struct Svm;

/// The `StateSave` part of the VMCB
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
    cr4: u64,
    cr3: u64,
    cr0: u64,
    dr7: u64,
    dr6: u64,
    rflags: u64,
    rip: u64,
    reserved_4: [u8; 0x1c0 - 0x180],
    instr_retired_ctr: u64,
    perf_ctr_global_sts: u64,
    perf_ctr_global_ctl: u64,
    reserved_5: [u8; 0x1d8 - 0x1d7],
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
    cr2: u64,
    reserved_6: [u8; 0x268 - 0x248],
    g_pat: u64,
    dbg_ctl: u64,
    br_from: u64,
    br_to: u64,
    last_exception_from: u64,
    last_exception_to: u64,
    dbg_extn_ctl: u64,
    reserved_7: [u8; 72],
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

/// The `Control` part of the VMCB
#[repr(C, packed)]
struct ControlArea {
    intercept_cr_reads: u16,
    intercept_cr_writes: u16,
    intercept_dr_reads: u16,
    intercept_dr_writes: u16,
    intercept_exceptions: u32,
    intercept_insts_1: u32,
    intercept_insts_2: u32,
    intercept_insts_3: u32,
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
    exitintinfo: u64,
    partially_reserved_12: u64,
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

/// The VMCB structure.
///
/// Each VM has one, so HAV could be used
#[repr(C, packed(0x1000))]
pub struct Vmcb {
    control: ControlArea,
    state_save: StateSaveArea,
}

#[derive(Debug, Clone, Copy)]
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

    // XXX: Fix this
    /// The error code QEMU returns instead of `Invalid`
    _BadInvalid = 0xffff_ffff,
}

impl Vmcb {
    #[inline]
    fn uninit() -> Self {
        unsafe { core::mem::zeroed() }
    }

    /// Initializes the guest state of the VMCB.
    ///
    /// The processor will load these fields when `VMLOAD` is executed.
    ///
    /// NOTE: Not every combination of fields is valid. See the AMD APM Vol 2, `Canonicalization
    /// and Consistency Checks`
    fn init_guest_state(&mut self) {
        println!("here!");
        let gdt_ptr: *mut Gdt = {
            let virt_addr: VirtAddr = Gdt::read_gdtr().into();
            virt_addr.into()
        };

        println!("that's the GDTR: {:?}", gdt_ptr);

        let gdt = unsafe { gdt_ptr.as_mut().unwrap() };

        self.state_save.cs = gdt.read_full_selector(read_cs());
        self.state_save.rip = __isr_stub_generic_irq_isr as u64;
        self.state_save.rflags = cpu::get_rflags();
        self.state_save.ss = gdt.read_full_selector(read_ss());
        self.state_save.rsp = unsafe {
            let ret: u64;
            asm!(
                "mov {}, rsp",
                out(reg) ret,
            );
            ret
        };
        self.state_save.cr0 = read_cr!(0);
        self.state_save.cr2 = read_cr!(2);
        self.state_save.cr3 = read_cr!(3);
        self.state_save.cr4 = read_cr!(4);
        self.state_save.efer = unsafe {
            let foo = rdmsr(AmdMsr::Efer);

            (foo.0 as u64) | (foo.1 as u64 >> 32)
        };
        self.state_save.idtr = Idt::read_idtr().into();
        self.state_save.gdtr = Idt::read_gdtr().into();
        self.state_save.es = gdt.read_full_selector(read_es());
        self.state_save.ds = gdt.read_full_selector(read_ds());
        self.state_save.dr6 = read_dr6() as u64;
        self.state_save.dr7 = read_dr7() as u64;
        self.state_save.cpl = 0;

        // TODO: Not sure about RAX
    }

    fn handle_vmexit(&mut self) {
        {
            let foo = self.control.exitcode;
            println!("this is the error code {:?}", foo);
            unsafe {
                asm!("hlt");
            };
        }
    }
}

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

    // Allocate the physical page for the host state area.
    // We do this seperatly and not using `allocate_pages` because we need to use both the physical
    // address and the pointer, so it's faster doing that seperatly
    let host_state_page = pmm::get()
        .allocate(NonZero::new(1).unwrap(), NonZero::new(1).unwrap())
        .unwrap();

    // Getting rid of stale data
    unsafe {
        // Map the physical page so we can write to it
        let host_state_ptr: *mut u32 = map_page(host_state_page, Entry::FLAG_RW).into();

        memset(host_state_ptr.cast::<u8>(), 0x0, BASIC_PAGE_SIZE);
    };

    unsafe {
        // Breaking the physical address of the page into parts, so we can write it to the MSR
        let low = (host_state_page.0 & 0xffff_ffff) as u32;
        let high = ((host_state_page.0 >> 32) & 0xffff_ffff) as u32;

        // Set the host state save area
        wrmsr(AmdMsr::VmHsavePa, low, high);
    };
}

/// Enables the option to enter SVM operation.
fn enable() {
    const SVME_BIT: u32 = 1 << 12;

    check_support();
    check_firmware_disabled();

    unsafe {
        let (mut low, high) = rdmsr(AmdMsr::Efer);
        low |= SVME_BIT;
        wrmsr(AmdMsr::Efer, low, high);
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

#[inline]
unsafe fn vmrun(vmcb: PhysAddr) {
    assert!(vmcb.0 % BASIC_PAGE_SIZE == 0);

    unsafe {
        asm!(
            "vmrun rax",
            in("rax") vmcb.0,
        );
    };
}

/// Perform a check to see if virtualization is disabled by the firmware.
fn check_firmware_disabled() {
    // TODO: COrrect this shit
    // // TODO: Perform a check for TPM as well
    // const SVML_BIT: u32 = 1 << 2;
    //
    // unsafe {
    //     assert!(
    //         __cpuid(0x8000_000A).edx & SVML_BIT != 0,
    //         "SVM is disabled by firmware and thus cannot be enabled"
    //     );
    // };
}

impl VirtTech for Svm {
    type VesselControlBlock = Vmcb;

    fn start() {
        enable();
        init_host_state();

        log_info!("Started SVM operation successfully");
    }

    fn stop() {}
}

impl Vesselable for Vmcb {
    fn new() -> Box<Self, &'static SlabAllocator<Self>> {
        // TODO: Might be better to use a custom slab allocator for this?
        const PAGE_COUNT: usize = size_of::<Vmcb>().div_ceil(BASIC_PAGE_SIZE);

        // TODO: Fix this slab allocator. It's bad
        let vmcb = Box::new_in(Self::uninit(), &VMCB_ALLOCATOR);

        println!("that's the address {:?}", ptr::from_ref(vmcb.as_ref()));

        vmcb
    }

    fn load(&mut self) {
        let ptr = ptr::from_mut(self);
        let phys_addr = translate(ptr.into()).unwrap();

        unsafe {
            vmrun(phys_addr);
        };

        self.handle_vmexit();
    }
}

impl SlabAllocatable for Vmcb {
    fn initalizer() {}
}
