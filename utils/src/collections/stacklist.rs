//! A simple, unidirectional linked list

use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

#[cfg(not(test))]
use alloc::boxed::Box;
#[cfg(not(test))]
extern crate alloc;
#[cfg(test)]
use std::boxed::Box;

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
    data: T,
    next: Option<NonNull<Node<T>>>,
}

#[derive(Debug)]
pub struct StackList<T> {
    len: usize,
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
}

impl<T> Node<T> {
    #[inline]
    pub const fn new(data: T) -> Self {
        Node { data, next: None }
    }

    #[inline]
    pub const fn offset_of() -> isize {
        core::mem::offset_of!(Node<T>, data) as isize
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

impl<T> StackList<T> {
    #[inline]
    pub const fn new() -> Self {
        StackList {
            head: None,
            tail: None,
            len: 0,
        }
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        self.pop_front_node().map(|node| node.data)
    }

    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        self.pop_back_node().map(|node| node.data)
    }

    #[inline]
    pub fn push_front(&mut self, data: T) {
        unsafe { self.push_front_node(Box::leak(Box::new(Node::new(data))).into()) }
    }

    #[inline]
    pub fn push_back(&mut self, data: T) {
        unsafe { self.push_back_node(Box::leak(Box::new(Node::new(data))).into()) }
    }

    pub unsafe fn push_front_node(&mut self, mut node: NonNull<Node<T>>) {
        unsafe { node.as_mut().next = self.head };
        self.head = Some(node);
        self.len += 1;
        if self.len < 2 {
            self.tail = self.head;
        }
    }

    pub fn pop_front_node(&mut self) -> Option<Box<Node<T>>> {
        self.head.map(|node| {
            self.len -= 1;
            self.head = unsafe { node.as_ref().next };
            if self.len < 2 {
                self.tail = self.head;
            }
            // SAFETY: node is valid and not aliased.
            unsafe { Box::from_raw(node.as_ptr()) }
        })
    }

    #[inline]
    pub fn front(&self) -> Option<&T> {
        self.head
            .as_ref()
            .map(|node| unsafe { &node.as_ref().data })
    }

    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.head
            .as_mut()
            .map(|node| unsafe { &mut node.as_mut().data })
    }

    #[inline]
    pub fn back(&self) -> Option<&T> {
        self.tail
            .as_ref()
            .map(|node| unsafe { &node.as_ref().data })
    }

    #[inline]
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.tail
            .as_mut()
            .map(|node| unsafe { &mut node.as_mut().data })
    }

    #[inline]
    pub fn pop_back_node(&mut self) -> Option<Box<Node<T>>> {
        let prev_tail = {
            let mut prev_tail = self.head;
            while let Some(node) = prev_tail {
                if unsafe { node.as_ref().next == self.tail } {
                    break;
                }
                prev_tail = unsafe { node.as_ref().next };
            }
            prev_tail
        };

        self.tail.map(|node| {
            self.len -= 1;
            if let Some(mut prev_tail) = prev_tail {
                unsafe { prev_tail.as_mut().next = None };
                self.tail = Some(prev_tail);
            } else {
                self.head = None;
                self.tail = None;
            }
            // SAFETY: node is valid and not aliased.
            unsafe { Box::from_raw(node.as_ptr()) }
        })
    }

    pub unsafe fn push_back_node(&mut self, node: NonNull<Node<T>>) {
        if let Some(mut old_tail) = self.tail {
            unsafe { old_tail.as_mut().next = Some(node) };
        }

        self.tail = Some(node);
        self.len += 1;
        if self.len < 2 {
            self.head = self.tail;
        }
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
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn iter_node(&self) -> IterNode<T> {
        IterNode {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_node_mut(&mut self) -> IterNodeMut<T> {
        IterNodeMut {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter(&self) -> Iter<T> {
        Iter {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }
}

impl<T> Drop for StackList<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop_front_node() {}
    }
}

impl<'a, T> Iterator for IterNode<'a, T> {
    type Item = &'a Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.head.map(|node| {
            self.len -= 1;
            self.head = unsafe { node.as_ref().next };
            // SAFETY: node is valid and not aliased.
            unsafe { node.as_ref() }
        })
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
        self.head.map(|mut node| {
            self.len -= 1;
            self.head = unsafe { node.as_ref().next };
            // SAFETY: node is valid and not aliased.
            unsafe { node.as_mut() }
        })
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
        self.head.map(|node| {
            self.len -= 1;
            self.head = unsafe { node.as_ref().next };
            // SAFETY: node is valid and not aliased.
            unsafe { &node.as_ref().data }
        })
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
        self.head.map(|mut node| {
            self.len -= 1;
            self.head = unsafe { node.as_ref().next };
            // SAFETY: node is valid and not aliased.
            unsafe { &mut node.as_mut().data }
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

mod tests {
    #[test]
    fn test_stacklist() {
        let mut list = super::StackList::new();
        assert_eq!(list.len(), 0);
        assert_eq!(list.is_empty(), true);

        list.push_front(1);
        list.push_front(2);
        list.push_front(3);
        list.push_front(4);
        list.push_front(5);

        assert_eq!(list.len(), 5);
        assert_eq!(list.is_empty(), false);

        assert_eq!(list.pop_front(), Some(5));
        assert_eq!(list.pop_front(), Some(4));
        assert_eq!(list.pop_front(), Some(3));
        assert_eq!(list.pop_front(), Some(2));
        assert_eq!(list.pop_front(), Some(1));
        assert_eq!(list.pop_front(), None);

        assert_eq!(list.len(), 0);
        assert_eq!(list.is_empty(), true);

        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        list.push_back(4);
        list.push_back(5);

        assert_eq!(list.len(), 5);
        assert_eq!(list.is_empty(), false);

        assert_eq!(list.pop_back(), Some(5));
        assert_eq!(list.pop_back(), Some(4));
        assert_eq!(list.pop_back(), Some(3));

        assert_eq!(list.len(), 2);
        assert_eq!(list.is_empty(), true);
    }

    #[test]
    fn test_stacklist_iter() {
        let mut list = super::StackList::new();
        list.push_front(1);
        list.push_front(2);
        list.push_front(3);
        list.push_front(4);
        list.push_front(23);
        list.push_front(673);
        list.push_front(435);
        list.push_front(56453);
        list.push_front(3435);
        list.push_front(21545);
        list.push_front(2452);
        list.push_front(353456);

        let mut iter = list.iter();
        assert_eq!(*iter.next().unwrap(), 353456);
        assert_eq!(*iter.next().unwrap(), 2452);
        assert_eq!(*iter.next().unwrap(), 21545);
        assert_eq!(*iter.next().unwrap(), 3435);
        assert_eq!(*iter.next().unwrap(), 56453);
        assert_eq!(*iter.next().unwrap(), 435);
        assert_eq!(*iter.next().unwrap(), 673);
        assert_eq!(*iter.next().unwrap(), 23);
        assert_eq!(*iter.next().unwrap(), 4);
        assert_eq!(*iter.next().unwrap(), 3);
        assert_eq!(*iter.next().unwrap(), 2);
        assert_eq!(*iter.next().unwrap(), 1);
    }

    #[test]
    fn test_stacklist_string() {
        let mut list = super::StackList::new();
        list.push_back("hello");
        list.push_back("there");
        list.push_back("general");
        list.push_back("kenobi");

        assert_eq!(list.front(), Some(&"hello"));
        assert_eq!(list.back(), Some(&"kenobi"));
        assert_eq!(list.len(), 4);
        assert_eq!(list.pop_front(), Some("hello"));
        assert_eq!(list.pop_back(), Some("kenobi"));

        assert_eq!(list.front(), Some(&"there"));
        assert_eq!(list.back(), Some(&"general"));
        assert_eq!(list.len(), 2);
        assert_eq!(list.pop_front(), Some("there"));
        assert_eq!(list.pop_back(), Some("general"));

        assert_eq!(list.front(), None);
        assert_eq!(list.back(), None);
        assert_eq!(list.len(), 0);
    }
}
