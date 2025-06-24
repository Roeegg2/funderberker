/// A faster, simpler `OnceCell` alternative *when you know what you're doing* - that is when you
/// can **100%** guarantee that the safety rules apply. If you can't, use the regular `OnceCell` instead.
use core::cell::SyncUnsafeCell;

#[repr(transparent)]
pub struct FastLazyStatic<T>
where
    T: Copy + PartialEq,
{
    data: SyncUnsafeCell<T>,
}

/// This simply creates two functions: a setter and a getter.
///
/// The setter should be called only once, and thus marked as unsafe.
/// The getter just returns the value
impl<T> FastLazyStatic<T>
where
    T: Copy + PartialEq,
{
    #[inline]
    pub const fn new(uninit: T) -> Self {
        Self {
            data: SyncUnsafeCell::new(uninit),
        }
    }

    #[inline]
    pub unsafe fn set(&self, data: T) {
        unsafe {
            let foo = self.data.get().as_mut().unwrap();

            // TODO: Might have to remove this since sometimes the UNINT type can be valid
            // Sanity checking to make sure the value wasn't already set
            // sanity_assert!(*foo == T::UNINIT);
            *foo = data;
        }
    }

    #[inline]
    pub fn get(&self) -> T {
        unsafe {
            let foo = *self.data.get();

            // TODO: Might have to remove this since sometimes the UNINT type can be valid
            // Making sure the value has indeed been set
            // sanity_assert!(foo != T::UNINIT);

            foo
        }
    }
}
