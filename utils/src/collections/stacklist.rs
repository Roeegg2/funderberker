//! A simple, safe stack-based linked list

use core::{
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use alloc::boxed::Box;

/// Iterator that yields references to the data
pub struct Iter<'a, T> {
    current: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a Node<T>>,
}

/// Iterator that yields mutable references to the data
pub struct IterMut<'a, T> {
    current: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a mut Node<T>>,
}

/// Iterator that yields references to the nodes themselves
pub struct NodeIter<'a, T> {
    current: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a Node<T>>,
}

/// Iterator that yields mutable references to the nodes themselves
pub struct NodeIterMut<'a, T> {
    current: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a mut Node<T>>,
}

/// A node in the linked list
#[derive(Debug)]
pub struct Node<T> {
    pub data: T,
    next: Option<NonNull<Node<T>>>,
}

/// A stack-based linked list (LIFO - Last In, First Out)
pub struct StackList<T> {
    head: Option<NonNull<Node<T>>>,
    len: usize,
}

impl<T> Node<T> {
    /// Create a new node with the given data
    #[inline]
    pub const fn new(data: T) -> Self {
        Node { data, next: None }
    }

    /// Get the next node, if any
    #[inline]
    pub fn next(&self) -> Option<&Node<T>> {
        self.next.map(|ptr| unsafe { ptr.as_ref() })
    }
}

impl<T> Deref for Node<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Node<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T> Default for StackList<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Send and Sync implementations - safe because we own all nodes
unsafe impl<T: Send> Send for StackList<T> {}
unsafe impl<T: Sync> Sync for StackList<T> {}

impl<T> StackList<T> {
    /// Create a new empty stack list
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        StackList { head: None, len: 0 }
    }

    /// Push a new element onto the top of the stack
    #[inline]
    pub fn push(&mut self, data: T) {
        unsafe {
            self.push_node(Box::into_non_null(Box::new(Node::new(data))));
        }
    }

    /// Push a boxed node onto the top of the stack
    pub unsafe fn push_node(&mut self, mut node: NonNull<Node<T>>) {
        unsafe {
            node.as_mut().next = self.head;
        }
        self.head = Some(node);
        self.len += 1;
    }

    /// Pop an element from the top of the stack
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.pop_node().map(|node| unsafe {
            let ptr = node.as_ptr(); // *mut Node<T>
            let data = core::ptr::read(&(*ptr).data); // move T out without clone
            // Optional: deallocate node here if you're managing memory
            data
        })
    }

    /// Pop a node from the top of the stack
    pub fn pop_node(&mut self) -> Option<NonNull<Node<T>>> {
        self.head.map(|mut node| {
            self.head = unsafe { node.as_mut().next.take() };
            self.len -= 1;
            node
        })
    }

    /// Peek at the top element without removing it
    #[inline]
    #[must_use]
    pub fn peek(&self) -> Option<&T> {
        self.head.map(|head| unsafe { &head.as_ref().data })
    }

    /// Peek at the top element mutably without removing it
    #[inline]
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.head.map(|mut head| unsafe { &mut head.as_mut().data })
    }

    /// Remove and return the element at the given index
    pub fn remove_at(&mut self, index: usize) -> Option<NonNull<Node<T>>> {
        if index >= self.len {
            return None;
        }

        if index == 0 {
            return self.pop_node();
        }

        // Find the node before the one we want to remove
        let mut current = self.head?;
        for _ in 0..(index - 1) {
            current = unsafe { current.as_ref().next? };
        }

        // Remove the next node
        unsafe {
            let mut node_to_remove = current.as_ref().next?;
            current.as_mut().next = node_to_remove.as_mut().next;
            self.len -= 1;
            Some(node_to_remove)
        }
    }

    /// Get an element at the given index without removing it
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }

        let mut current = self.head?;
        for _ in 0..index {
            current = unsafe { current.as_ref().next? };
        }

        Some(unsafe { &current.as_ref().data })
    }

    /// Get a mutable reference to an element at the given index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        let mut current = self.head?;
        for _ in 0..index {
            current = unsafe { current.as_ref().next? };
        }

        Some(unsafe { &mut current.as_mut().data })
    }

    /// Check if the list is empty
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Get the number of elements in the list
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Clear all elements from the list
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Create an iterator over references to the data
    #[inline]
    #[must_use]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            current: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Create an iterator over mutable references to the data
    #[inline]
    #[must_use]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            current: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Create an iterator over references to the nodes
    #[inline]
    #[must_use]
    pub fn iter_node(&self) -> NodeIter<'_, T> {
        NodeIter {
            current: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Create an iterator over mutable references to the nodes
    #[inline]
    #[must_use]
    pub fn iter_node_mut(&mut self) -> NodeIterMut<'_, T> {
        NodeIterMut {
            current: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Convert the list into a Vec
    pub fn into_vec(mut self) -> alloc::vec::Vec<T> {
        let mut vec = alloc::vec::Vec::with_capacity(self.len);
        while let Some(item) = self.pop() {
            vec.push(item);
        }
        vec.reverse(); // Maintain original order
        vec
    }

    /// Retain only elements that satisfy the predicate
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let mut current = &mut self.head;

        while let Some(mut node_ptr) = *current {
            let node = unsafe { node_ptr.as_ref() };
            if f(&node.data) {
                current = unsafe { &mut node_ptr.as_mut().next };
            } else {
                let mut removed_node = unsafe { Box::from_raw(node_ptr.as_ptr()) };
                *current = removed_node.next.take();
                self.len -= 1;
            }
        }
    }
}

impl<T> Drop for StackList<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

// Iterator implementations
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.current.map(|current| {
            let node = unsafe { current.as_ref() };
            self.current = node.next;
            self.remaining -= 1;
            &node.data
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.current.map(|mut current| {
            let node = unsafe { current.as_mut() };
            self.current = node.next;
            self.remaining -= 1;
            &mut node.data
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}

impl<'a, T> Iterator for NodeIter<'a, T> {
    type Item = &'a Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.current.map(|current| {
            let node = unsafe { current.as_ref() };
            self.current = node.next;
            self.remaining -= 1;
            node
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for NodeIter<'a, T> {}

// IntoIterator implementations
impl<T> IntoIterator for StackList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { list: self }
    }
}

impl<'a, T> Iterator for NodeIterMut<'a, T> {
    type Item = &'a mut Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.current.map(|mut current| {
            let node = unsafe { current.as_mut() };
            self.current = node.next;
            self.remaining -= 1;
            node
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for NodeIterMut<'a, T> {}

impl<'a, T> IntoIterator for &'a StackList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut StackList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Owned iterator that consumes the list
pub struct IntoIter<T> {
    list: StackList<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.list.pop()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.list.len, Some(self.list.len))
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {}

// Collection traits
impl<T> FromIterator<T> for StackList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut list = StackList::new();
        for item in iter {
            list.push(item);
        }
        list
    }
}

impl<T> Extend<T> for StackList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }
}

// Display and Debug
impl<T> Debug for StackList<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Clone for StackList<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut new_list = StackList::new();
        let items: alloc::vec::Vec<_> = self.iter().cloned().collect();
        for item in items.into_iter().rev() {
            new_list.push(item);
        }
        new_list
    }
}

impl<T> PartialEq for StackList<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<T> Eq for StackList<T> where T: Eq {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    #[test]
    fn test_new_list_is_empty() {
        let list: StackList<i32> = StackList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(list.peek().is_none());
    }

    #[test]
    fn test_push_and_peek() {
        let mut list = StackList::new();

        list.push(1);
        assert!(!list.is_empty());
        assert_eq!(list.len(), 1);
        assert_eq!(list.peek(), Some(&1));

        list.push(2);
        assert_eq!(list.len(), 2);
        assert_eq!(list.peek(), Some(&2));

        list.push(3);
        assert_eq!(list.len(), 3);
        assert_eq!(list.peek(), Some(&3));
    }

    #[test]
    fn test_pop() {
        let mut list = StackList::new();

        assert!(list.pop().is_none());

        list.push(1);
        list.push(2);
        list.push(3);

        assert_eq!(list.pop(), Some(3));
        assert_eq!(list.len(), 2);
        assert_eq!(list.peek(), Some(&2));

        assert_eq!(list.pop(), Some(2));
        assert_eq!(list.len(), 1);
        assert_eq!(list.peek(), Some(&1));

        assert_eq!(list.pop(), Some(1));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());

        assert!(list.pop().is_none());
    }

    #[test]
    fn test_get_and_get_mut() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        assert_eq!(list.get(0), Some(&3));
        assert_eq!(list.get(1), Some(&2));
        assert_eq!(list.get(2), Some(&1));
        assert_eq!(list.get(3), None);

        if let Some(val) = list.get_mut(1) {
            *val = 42;
        }
        assert_eq!(list.get(1), Some(&42));
    }

    #[test]
    fn test_retain() {
        let mut list: StackList<i32> = [1, 2, 3, 4, 5].iter().cloned().collect();
        list.retain(|&x| x % 2 == 0);

        let items: Vec<_> = list.iter().cloned().collect();
        assert_eq!(items, vec![4, 2]);
    }

    #[test]
    fn test_clear() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        assert_eq!(list.len(), 3);
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn test_into_vec() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        let vec = list.into_vec();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_clone() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        let cloned = list.clone();
        assert_eq!(list, cloned);
        assert_eq!(list.len(), cloned.len());
    }

    #[test]
    fn test_from_iterator() {
        let list: StackList<i32> = vec![1, 2, 3].into_iter().collect();
        let items: Vec<_> = list.iter().cloned().collect();
        assert_eq!(items, vec![3, 2, 1]); // Stack order
    }

    #[test]
    fn test_into_iterator() {
        let list: StackList<i32> = vec![1, 2, 3].into_iter().collect();
        let items: Vec<_> = list.into_iter().collect();
        assert_eq!(items, vec![3, 2, 1]); // Stack order
    }

    #[test]
    fn test_iterator_safety() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        // This should work fine - immutable borrows
        let iter1 = list.iter();
        let iter2 = list.iter();

        let sum1: i32 = iter1.sum();
        let sum2: i32 = iter2.sum();
        assert_eq!(sum1, sum2);
        assert_eq!(sum1, 6);
    }

    #[test]
    fn test_exact_size_iterator() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        let mut iter = list.iter();
        assert_eq!(iter.len(), 3);
        iter.next();
        assert_eq!(iter.len(), 2);
        iter.next();
        assert_eq!(iter.len(), 1);
        iter.next();
        assert_eq!(iter.len(), 0);
    }

    #[test]
    fn test_iter_node() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        let mut node_count = 0;
        for node in list.iter_node() {
            assert!(node.data > 0);
            node_count += 1;
        }
        assert_eq!(node_count, 3);

        // Test size_hint
        let mut iter = list.iter_node();
        assert_eq!(iter.size_hint(), (3, Some(3)));
        iter.next();
        assert_eq!(iter.size_hint(), (2, Some(2)));
    }

    #[test]
    fn test_iter_node_mut() {
        let mut list = StackList::new();
        list.push(1);
        list.push(2);
        list.push(3);

        // Modify through mutable node iterator
        for node in list.iter_node_mut() {
            node.data *= 10;
        }

        let items: Vec<&i32> = list.iter().collect();
        assert_eq!(items, vec![&30, &20, &10]);

        // Test size_hint
        let mut iter = list.iter_node_mut();
        assert_eq!(iter.size_hint(), (3, Some(3)));
        iter.next();
        assert_eq!(iter.size_hint(), (2, Some(2)));
    }
}
