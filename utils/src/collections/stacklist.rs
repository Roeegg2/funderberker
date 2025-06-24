//! A simple, unidirectional linked list

use alloc::boxed::Box;
use core::{
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

pub struct Iter<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<&'a Node<T>>,
}

pub struct IterMut<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<&'a mut Node<T>>,
}

pub struct IterNode<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<&'a Node<T>>,
}

pub struct IterNodeMut<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<&'a mut Node<T>>,
}

#[derive(Debug)]
pub struct Node<T> {
    pub data: T,
    next: Option<NonNull<Node<T>>>,
}

pub struct StackList<T> {
    len: usize,
    head: Option<NonNull<Node<T>>>,
}

impl<T> Node<T> {
    #[inline]
    pub const fn new(data: T) -> Self {
        Node { data, next: None }
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

impl<T> StackList<T> {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        StackList { head: None, len: 0 }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.pop_node().map(|node| node.data)
    }

    #[inline]
    pub fn push(&mut self, data: T) {
        unsafe { self.push_node(Box::leak(Box::new(Node::new(data))).into()) }
    }

    pub unsafe fn push_node(&mut self, mut node: NonNull<Node<T>>) {
        unsafe { node.as_mut().next = self.head };
        self.head = Some(node);
        self.len += 1;
    }

    pub fn pop_node(&mut self) -> Option<Box<Node<T>>> {
        self.head.map(|node| {
            let node = unsafe { Box::from_raw(node.as_ptr()) };
            self.head = node.as_ref().next;
            self.len -= 1;
            node
        })
    }

    pub fn pop_into(&mut self, other: &mut Self) {
        let node = self.pop_node();
        if let Some(node) = node {
            // SAFETY: The node is valid and not aliased.
            unsafe { other.push_node(Box::into_non_null(node)) };
        }
    }

    pub fn remove_into(&mut self, other: &mut Self, index: usize) {
        let node = self.remove_at(index);
        if let Some(node) = node {
            // SAFETY: The node is valid and not aliased.
            unsafe { other.push_node(Box::into_non_null(node)) };
        }
    }

    #[inline]
    #[must_use]
    pub fn peek(&self) -> Option<&T> {
        self.head
            .as_ref()
            .map(|node| unsafe { &node.as_ref().data })
    }

    #[inline]
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.head
            .as_mut()
            .map(|node| unsafe { &mut node.as_mut().data })
    }

    pub fn remove_at(&mut self, index: usize) -> Option<Box<Node<T>>> {
        if index >= self.len {
            return None;
        }

        let mut prev: Option<NonNull<Node<T>>> = None;
        let mut current = self.head;
        let mut i = 0;
        while let Some(node) = current {
            if i == index {
                if let Some(mut prev) = prev {
                    unsafe { prev.as_mut().next = node.as_ref().next };
                } else {
                    self.head = unsafe { node.as_ref().next };
                }
                self.len -= 1;
                return Some(unsafe { Box::from_raw(node.as_ptr()) });
            }
            prev = current;
            current = unsafe { node.as_ref().next };
            i += 1;
        }

        None
    }

    /// Check if the list is empty
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    #[must_use]
    pub fn iter_node(&self) -> IterNode<'_, T> {
        IterNode {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_node_mut(&mut self) -> IterNodeMut<'_, T> {
        IterNodeMut {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    #[must_use]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }
}

impl<T> Drop for StackList<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop_node() {}
    }
}

impl<'a, T> Iterator for IterNode<'a, T> {
    type Item = &'a Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len != 0 {
            return self.head.map(|node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { node.as_ref() }
            });
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> Iterator for IterNodeMut<'a, T> {
    type Item = &'a mut Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len != 0 {
            return self.head.map(|mut node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { node.as_mut() }
            });
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len != 0 {
            return self.head.map(|node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { &node.as_ref().data }
            });
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len != 0 {
            return self.head.map(|mut node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { &mut node.as_mut().data }
            });
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> Debug for StackList<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{
        format,
        string::{String, ToString},
        vec,
        vec::Vec,
    };

    use super::*;

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
        assert_eq!(list.peek(), Some(&2)); // Stack behavior - last pushed is first

        list.push(3);
        assert_eq!(list.len(), 3);
        assert_eq!(list.peek(), Some(&3));
    }

    #[test]
    fn test_pop() {
        let mut list = StackList::new();

        // Pop from empty list
        assert!(list.pop().is_none());

        list.push(1);
        list.push(2);
        list.push(3);

        // Pop in LIFO order
        assert_eq!(list.pop(), Some(3));
        assert_eq!(list.len(), 2);
        assert_eq!(list.peek(), Some(&2));

        assert_eq!(list.pop(), Some(2));
        assert_eq!(list.len(), 1);
        assert_eq!(list.peek(), Some(&1));

        assert_eq!(list.pop(), Some(1));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        assert!(list.peek().is_none());

        // Pop from empty list again
        assert!(list.pop().is_none());
    }

    #[test]
    fn test_peek_mut() {
        let mut list = StackList::new();

        // peek_mut on empty list
        assert!(list.peek_mut().is_none());

        list.push(42);

        if let Some(val) = list.peek_mut() {
            *val = 100;
        }

        assert_eq!(list.peek(), Some(&100));
        assert_eq!(list.pop(), Some(100));
    }

    #[test]
    fn test_remove_at() {
        let mut list = StackList::new();

        // Remove from empty list
        assert!(list.remove_at(0).is_none());

        list.push(1);
        list.push(2);
        list.push(3);
        list.push(4);
        // List is now: [4, 3, 2, 1] (head to tail)

        // Remove out of bounds
        assert!(list.remove_at(4).is_none());
        assert!(list.remove_at(100).is_none());

        // Remove from middle (index 1, which is 3)
        let removed = list.remove_at(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().data, 3);
        assert_eq!(list.len(), 3);

        // Remove from head (index 0, which is 4)
        let removed = list.remove_at(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().data, 4);
        assert_eq!(list.len(), 2);
        assert_eq!(list.peek(), Some(&2));

        // Remove from tail (index 1, which is 1)
        let removed = list.remove_at(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().data, 1);
        assert_eq!(list.len(), 1);

        // Remove last element
        let removed = list.remove_at(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().data, 2);
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn test_pop_into() {
        let mut list1 = StackList::new();
        let mut list2 = StackList::new();

        list1.push(1);
        list1.push(2);
        list1.push(3);

        // Pop from list1 into list2
        list1.pop_into(&mut list2);

        assert_eq!(list1.len(), 2);
        assert_eq!(list2.len(), 1);
        assert_eq!(list1.peek(), Some(&2));
        assert_eq!(list2.peek(), Some(&3));

        // Pop from empty list1
        list1.pop();
        list1.pop();
        list1.pop_into(&mut list2); // Should do nothing

        assert_eq!(list1.len(), 0);
        assert_eq!(list2.len(), 1);
    }

    #[test]
    fn test_remove_into() {
        let mut list1 = StackList::new();
        let mut list2 = StackList::new();

        list1.push(1);
        list1.push(2);
        list1.push(3);
        list1.push(4);

        // Remove from middle of list1 into list2
        list1.remove_into(&mut list2, 1);

        assert_eq!(list1.len(), 3);
        assert_eq!(list2.len(), 1);
        assert_eq!(list2.peek(), Some(&3));

        // Remove out of bounds - should do nothing
        list1.remove_into(&mut list2, 10);

        assert_eq!(list1.len(), 3);
        assert_eq!(list2.len(), 1);
    }

    #[test]
    fn test_iter() {
        let mut list = StackList::new();

        // Empty list iteration
        let items: Vec<&i32> = list.iter().collect();
        assert!(items.is_empty());

        list.push(1);
        list.push(2);
        list.push(3);

        let items: Vec<&i32> = list.iter().collect();
        assert_eq!(items, vec![&3, &2, &1]); // Head to tail order

        // Test size_hint
        let mut iter = list.iter();
        assert_eq!(iter.size_hint(), (3, Some(3)));
        iter.next();
        assert_eq!(iter.size_hint(), (2, Some(2)));
    }

    #[test]
    fn test_iter_mut() {
        let mut list = StackList::new();

        list.push(1);
        list.push(2);
        list.push(3);

        // Modify through mutable iterator
        for item in list.iter_mut() {
            *item *= 2;
        }

        let items: Vec<&i32> = list.iter().collect();
        assert_eq!(items, vec![&6, &4, &2]);

        // Test size_hint
        let mut iter = list.iter_mut();
        assert_eq!(iter.size_hint(), (3, Some(3)));
        iter.next();
        assert_eq!(iter.size_hint(), (2, Some(2)));
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

    #[test]
    fn test_node_deref() {
        let node = Node::new(42);
        assert_eq!(*node, 42); // Test Deref

        let mut node = Node::new(42);
        *node = 100; // Test DerefMut
        assert_eq!(*node, 100);
    }

    #[test]
    fn test_debug_formatting() {
        let mut list = StackList::new();

        // Empty list
        let debug_str = format!("{:?}", list);
        assert_eq!(debug_str, "[]");

        list.push(1);
        list.push(2);
        list.push(3);

        let debug_str = format!("{:?}", list);
        assert_eq!(debug_str, "[3, 2, 1]");
    }

    #[test]
    fn test_drop_behavior() {
        // This test ensures that the Drop implementation doesn't panic
        // and properly cleans up all nodes
        {
            let mut list = StackList::new();
            for i in 0..1000 {
                list.push(i);
            }
            // List goes out of scope here and should be properly dropped
        }
        // If we reach this point without panicking, Drop worked correctly
    }

    #[test]
    fn test_with_different_types() {
        // Test with String
        let mut string_list = StackList::new();
        string_list.push("hello".to_string());
        string_list.push("world".to_string());

        assert_eq!(string_list.peek(), Some(&"world".to_string()));
        assert_eq!(string_list.pop(), Some("world".to_string()));

        // Test with custom struct
        #[derive(Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
        }

        let mut person_list = StackList::new();
        person_list.push(Person {
            name: "Alice".to_string(),
            age: 30,
        });
        person_list.push(Person {
            name: "Bob".to_string(),
            age: 25,
        });

        assert_eq!(person_list.len(), 2);
        let bob = person_list.pop().unwrap();
        assert_eq!(bob.name, "Bob");
        assert_eq!(bob.age, 25);
    }

    #[test]
    fn test_large_list_operations() {
        let mut list = StackList::new();
        let n = 10000;

        // Push many items
        for i in 0..n {
            list.push(i);
        }
        assert_eq!(list.len(), n);

        // Remove from various positions
        for i in 0..100 {
            list.remove_at(i * 10);
        }

        // Pop remaining items
        let mut popped_count = 0;
        while list.pop().is_some() {
            popped_count += 1;
        }

        assert!(list.is_empty());
        assert!(popped_count > 0);
    }

    #[test]
    fn test_iterator_chain() {
        let mut list = StackList::new();
        for i in 1..=5 {
            list.push(i);
        }

        // Test chaining iterator operations
        let sum: i32 = list.iter().map(|&x| x * 2).sum();
        assert_eq!(sum, 30); // (5+4+3+2+1) * 2 = 15 * 2 = 30

        let filtered: Vec<i32> = list.iter().filter(|&&x| x % 2 == 0).map(|&x| x).collect();
        assert_eq!(filtered, vec![4, 2]);
    }

    #[test]
    fn test_concurrent_iteration() {
        let mut list = StackList::new();
        for i in 1..=3 {
            list.push(i);
        }

        // Test that we can create multiple iterators
        let iter1 = list.iter();
        let iter2 = list.iter();

        let items1: Vec<&i32> = iter1.collect();
        let items2: Vec<&i32> = iter2.collect();

        assert_eq!(items1, items2);
        assert_eq!(items1, vec![&3, &2, &1]);
    }
}
