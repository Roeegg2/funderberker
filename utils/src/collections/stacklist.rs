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

    pub fn pop_back_node(&mut self) -> Option<Box<Node<T>>> {
        self.tail.map(|node| {
            self.len -= 1;
            self.tail = {
                // XXX: THIS CODE MIGHT BE WRONG!
                let mut current = self.head;
                while let Some(next) = current.map(|node| unsafe { node.as_ref().next }) {
                    if next == self.tail {
                        break;
                    }
                    current = next;
                }
                current
            };
            if self.len < 2 {
                self.head = self.tail;
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

//impl<T> Debug

impl<'a, T> Iterator for Iter<'a, T> {
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

impl<'a, T> Iterator for IterMut<'a, T> {
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
