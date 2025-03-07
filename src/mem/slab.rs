//! A simple slab allocator for VMM heap
// 
// struct Cache {
//      ptr
//      bitmap (for now array of Options)
//      len // for optimization
//      next: Option<NonNull<Cache>>
// }
//
// struct SlabAllocator {
//      cache_head: Option<NonNull<Cache>>
// }
//
