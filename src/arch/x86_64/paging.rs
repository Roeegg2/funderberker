//! The core x86_64 paging mechanism

#[cfg(all(feature = "paging_4", feature = "paging_5"))]
compiler_error!("Can't have both 4 level and 5 level paging. Choose one of the options");
#[cfg(none(feature = "paging_4", feature = "paging_5"))]
compiler_error!("No paging level is selected. Choose one of the options");
