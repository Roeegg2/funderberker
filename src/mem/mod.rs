mod pmm;

pub(self) type PageId = usize;

pub(self) fn page_id_to_addr(page_id: PageId) -> usize {
    page_id * 4096
}

pub(self) fn addr_to_page_id(addr: usize) -> Option<PageId> {
    if addr % 4096 != 0 {
        return None;
    }

    Some(addr / 4096)
}
