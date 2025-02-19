// TODO: Add support for 5 level paging

const PML4E_INIT_STATE: u64 = 0x0; // an init state. basically just mark the page as not present

#[repr(C, align(4096))]
struct Table([u64; 512]);

#[repr(transparent)]
struct Entry(u64);

impl Entry {
    const FLAG_P: u64 = 1 << 0; // present bit - 0 => not present. 1 => present
    const FLAG_RW: u64 = 1 << 1; // read/write - 0 => just read. 1 => read + write
    const FLAG_US: u64 = 1 << 2; // user/supervisor - 0 => only CPL0,1,2. 1 => CPL3 as well 
    const FLAG_PWT: u64 = 1 << 3; // page-level writethrough - 0 => writeback. 1 => writethough caching. 
    const FLAG_PCD: u64 = 1 << 4; // page-level cache disable - 0 => cacheable. 1 => non cacheable.
    const FLAG_A: u64 = 1 << 5; // accessed - 0 => not accessed yet. 1 => page was read/writted to.
    const _FLAG_IGN: u64 = 1 << 6; 
    const FLAG_D: u64 = 1 << 6; // (on PTE only!) dirty - 0 => page not written to. 1 => page was written to.
    const FLAG_PS: u64 = 1 << 7; // (on PDE only!) page size - 0 => page is 4KB. 1 => page is 2MB.
                                 // should be set to 0 for all other tables
    const FLAG_PAT: u64 = 1 << 7; // 
    const _FLAG_MBZ: u64 = 0b11 << 7; // (on PML4E/PML5E only!)
    const _FLAG_IGN_2: u64 = 1 << 8; // on PDE/PDPE only! 
    const FLAG_G: u64 = 1 << 8; // on PTE only!
    const FLAG_AVL: u64 = 0b111 << 9;

    const fn set_flag(&mut self, flag: u64) {
        self.0 |= flag;
    }

    const fn clear_flag(&mut self, flag: u64) {
        self.0 &= !flag;
    }
}

fn check_paging_support() {
    // UEFI puts us in Long mode, so PG and PAE should already be enabled
    // we don't care about PSE
}

pub fn setup_paging() {
    check_paging_support();
}

pub fn allocate_page() {

}

pub fn free_page() {

}
