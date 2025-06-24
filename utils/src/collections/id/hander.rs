//! Simple wrapper for an ID bump allocator

use crate::sync::spinlock::SpinLockable;

use super::Id;

/// A simple ID allocator that just hands out IDs from a virtually infinite pool, not requiring a
/// free
pub struct IdHander {
    /// The next ID to be allocated
    next: Id,
    /// The maximum ID that can be allocated.
    ///
    /// The allocator will panic once `next` exceeds this value.
    max: Id,
}

impl IdHander {
    // TODO: Remove this and use the `Default` when const default is possible
    /// Return an uninitialized `IdHander`.
    pub const fn uninit() -> Self {
        Self {
            next: Id(0),
            max: Id(0),
        }
    }

    /// Creates a new `IdHander` starting from the given ID.
    #[inline]
    pub const fn new_starting_from(start_id: Id, max_id: Id) -> Self {
        Self {
            next: start_id,
            max: max_id,
        }
    }

    /// Creates a new `IdHander` starting from 0.
    #[inline]
    pub const fn new(max_id: Id) -> Self {
        Self::new_starting_from(Id(0), max_id)
    }

    /// Handout the next ID
    #[inline]
    #[must_use]
    pub fn handout(&mut self) -> Option<Id> {
        self.handout_and_skip(1)
    }

    /// Handout the next ID and skip `count` IDs
    #[inline]
    pub fn handout_and_skip(&mut self, count: usize) -> Option<Id> {
        if self.next.0 > self.max.0 {
            return None; // Exhausted
        }

        let handed_out = self.next;
        self.next.0 += count;

        Some(handed_out)
    }

    /// Get
    #[inline]
    pub const fn peek_next(&self) -> Id {
        self.next
    }
}

impl SpinLockable for IdHander {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_id_hander() {
        let hander = IdHander::new(Id(100));
        assert_eq!(hander.peek_next(), Id(0));
    }

    #[test]
    fn test_new_starting_from() {
        let hander = IdHander::new_starting_from(Id(10), Id(100));
        assert_eq!(hander.peek_next(), Id(10));
    }

    #[test]
    fn test_uninit_id_hander() {
        let hander = IdHander::uninit();
        assert_eq!(hander.peek_next(), Id(0));
    }

    #[test]
    fn test_handout_single() {
        let mut hander = IdHander::new(Id(100));
        let handed_out = hander.handout().expect("Should handout ID");
        assert_eq!(handed_out, Id(0));
        assert_eq!(hander.peek_next(), Id(1));
    }

    #[test]
    fn test_handout_multiple() {
        let mut hander = IdHander::new(Id(100));

        let id1 = hander.handout().expect("Should handout first ID");
        let id2 = hander.handout().expect("Should handout second ID");
        let id3 = hander.handout().expect("Should handout third ID");

        assert_eq!(id1, Id(0));
        assert_eq!(id2, Id(1));
        assert_eq!(id3, Id(2));
        assert_eq!(hander.peek_next(), Id(3));
    }

    #[test]
    fn test_handout_with_custom_start() {
        let mut hander = IdHander::new_starting_from(Id(50), Id(100));

        let id1 = hander.handout().expect("Should handout first ID");
        let id2 = hander.handout().expect("Should handout second ID");

        assert_eq!(id1, Id(50));
        assert_eq!(id2, Id(51));
        assert_eq!(hander.peek_next(), Id(52));
    }

    #[test]
    fn test_handout_and_skip() {
        let mut hander = IdHander::new(Id(100));

        let id1 = hander
            .handout_and_skip(1)
            .expect("Should handout and skip 1");
        let id2 = hander
            .handout_and_skip(3)
            .expect("Should handout and skip 3");
        let id3 = hander
            .handout_and_skip(2)
            .expect("Should handout and skip 2");

        assert_eq!(id1, Id(0));
        assert_eq!(id2, Id(1));
        assert_eq!(id3, Id(4));
        assert_eq!(hander.peek_next(), Id(6));
    }

    #[test]
    fn test_handout_and_skip_zero() {
        let mut hander = IdHander::new(Id(100));

        // Skip 0 should behave like regular handout but without incrementing
        let id1 = hander
            .handout_and_skip(0)
            .expect("Should handout without skipping");
        assert_eq!(id1, Id(0));
        assert_eq!(hander.peek_next(), Id(0)); // Should not increment
    }

    #[test]
    fn test_handout_exhaustion() {
        let mut hander = IdHander::new(Id(2));

        let id1 = hander.handout().expect("Should handout first ID");
        let id2 = hander.handout().expect("Should handout second ID");
        let id3 = hander.handout().expect("Should handout third ID");

        assert_eq!(id1, Id(0));
        assert_eq!(id2, Id(1));
        assert_eq!(id3, Id(2));

        // Should be exhausted now
        let result = hander.handout();
        assert!(result.is_none());
    }

    #[test]
    fn test_handout_and_skip_exhaustion() {
        let mut hander = IdHander::new(Id(5));

        let id1 = hander.handout_and_skip(3).expect("Should handout and skip");
        assert_eq!(id1, Id(0));
        assert_eq!(hander.peek_next(), Id(3));

        let id2 = hander.handout_and_skip(2).expect("Should handout and skip");
        assert_eq!(id2, Id(3));
        assert_eq!(hander.peek_next(), Id(5));

        // Next handout should exceed max
        let result = hander.handout();
        assert_eq!(result, Some(Id(5)));
        assert!(hander.handout().is_none());
    }

    #[test]
    fn test_handout_large_skip_exhaustion() {
        let mut hander = IdHander::new(Id(10));

        // Skip a large amount that would exceed the max
        hander.handout_and_skip(15).unwrap();
        let result = hander.handout();
        assert!(result.is_none());
    }

    #[test]
    fn test_peek_next_doesnt_change_state() {
        let mut hander = IdHander::new(Id(100));

        // Peek multiple times
        assert_eq!(hander.peek_next(), Id(0));
        assert_eq!(hander.peek_next(), Id(0));

        // Handout should still give the first ID
        let handed_out = hander.handout().expect("Should handout ID");
        assert_eq!(handed_out, Id(0));

        // Now peek should show the next ID
        assert_eq!(hander.peek_next(), Id(1));
    }

    #[test]
    fn test_edge_case_zero_max() {
        let mut hander = IdHander::new(Id(0));
        let handed_out = hander.handout().expect("Should handout ID 0");
        assert_eq!(handed_out, Id(0));

        // Should be exhausted immediately
        let result = hander.handout();
        assert!(result.is_none());
    }

    #[test]
    fn test_edge_case_start_equals_max() {
        let mut hander = IdHander::new_starting_from(Id(5), Id(5));
        let handed_out = hander.handout().expect("Should handout single ID");
        assert_eq!(handed_out, Id(5));

        // Should be exhausted
        let result = hander.handout();
        assert!(result.is_none());
    }

    #[test]
    fn test_consecutive_exhaustion_behavior() {
        let mut hander = IdHander::new(Id(1));

        // Exhaust the hander
        hander.handout().expect("Should handout ID 0");
        hander.handout().expect("Should handout ID 1");

        // Multiple calls after exhaustion should all return None
        assert!(hander.handout().is_none());
    }
}
