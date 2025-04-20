//! A simple, unidirectional linked list

use core::{
    fmt::{self, Debug},
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

impl<T> StackList<T> {
    #[inline]
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

    #[inline]
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
        while let Some(_) = self.pop_node() {}
    }
}

impl<'a, T> Iterator for IterNode<'a, T> {
    type Item = &'a Node<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        } else {
            self.head.map(|node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { node.as_ref() }
            })
        }
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
        if self.len == 0 {
            return None;
        } else {
            self.head.map(|mut node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { node.as_mut() }
            })
        }
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
        if self.len == 0 {
            return None;
        } else {
            self.head.map(|node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { &node.as_ref().data }
            })
        }
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
        if self.len == 0 {
            return None;
        } else {
            self.head.map(|mut node| {
                self.len -= 1;
                self.head = unsafe { node.as_ref().next };
                // SAFETY: node is valid and not aliased.
                unsafe { &mut node.as_mut().data }
            })
        }
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
