//! A no_std-compatible doubly-linked list for kernel use.

use crate::lib::boxed::Box;
use core::ptr::NonNull;
use core::marker::PhantomData;

pub struct Node<T> {
    pub value: T,
    next: Option<NonNull<Node<T>>>,
    prev: Option<NonNull<Node<T>>>,
}

impl<T> Node<T> {
    pub fn new(value: T) -> Self {
        Node { value, next: None, prev: None }
    }
}

pub struct LinkedList<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<Box<Node<T>>>,
}

impl<T: PartialEq> LinkedList<T> {
    pub fn remove(&mut self, value: &T) -> bool {
        let mut current = self.head;

        while let Some(node_ptr) = current {
            unsafe {
                let node = &mut *node_ptr.as_ptr();
                if &node.value == value {
                    if let Some(prev) = node.prev {
                        (*prev.as_ptr()).next = node.next;
                    } else {
                        self.head = node.next;
                    }

                    if let Some(next) = node.next {
                        (*next.as_ptr()).prev = node.prev;
                    } else {
                        self.tail = node.prev;
                    }

                    Box::from_raw(node_ptr.as_ptr());
                    self.len -= 1;
                    return true;
                }
                current = node.next;
            }
        }

        false
    }
}

impl<T> LinkedList<T> {
    pub const fn new() -> Self {
        LinkedList {
            head: None,
            tail: None,
            len: 0,
            _marker: PhantomData,
        }
    }

    pub fn push_front(&mut self, value: T) {
        let mut new_node = Box::new(Node::new(value));
        let new_node_ptr = unsafe { NonNull::new_unchecked(Box::into_raw(new_node)) };

        if let Some(head) = self.head {
            unsafe {
                (*head.as_ptr()).prev = Some(new_node_ptr);
                (*new_node_ptr.as_ptr()).next = Some(head);
            }
        } else {
            self.tail = Some(new_node_ptr);
        }

        self.head = Some(new_node_ptr);
        self.len += 1;
    }

    pub fn push_back(&mut self, value: T) {
        let mut new_node = Box::new(Node::new(value));
        let new_node_ptr = unsafe { NonNull::new_unchecked(Box::into_raw(new_node)) };

        if let Some(tail) = self.tail {
            unsafe {
                (*tail.as_ptr()).next = Some(new_node_ptr);
                (*new_node_ptr.as_ptr()).prev = Some(tail);
            }
        } else {
            self.head = Some(new_node_ptr);
        }

        self.tail = Some(new_node_ptr);
        self.len += 1;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.head.map(|node_ptr| {
            let node = unsafe { Box::from_raw(node_ptr.as_ptr()) };
            self.head = node.next;

            if let Some(mut next) = self.head {
                unsafe { (*next.as_ptr()).prev = None; }
            } else {
                self.tail = None;
            }

            self.len -= 1;
            node.value
        })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|node_ptr| {
            let node = unsafe { Box::from_raw(node_ptr.as_ptr()) };
            self.tail = node.prev;

            if let Some(mut prev) = self.tail {
                unsafe { (*prev.as_ptr()).next = None; }
            } else {
                self.head = None;
            }

            self.len -= 1;
            node.value
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> LinkedListIter<'_, T> {
        LinkedListIter { current: self.head, _marker: PhantomData }
    }
}

pub struct LinkedListIter<'a, T> {
    current: Option<NonNull<Node<T>>>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for LinkedListIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.current.map(|node_ptr| {
            unsafe {
                let node = &*node_ptr.as_ptr();
                self.current = node.next;
                &node.value
            }
        })
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_operations() {
        let mut list = LinkedList::new();

        assert!(list.is_empty());

        list.push_back(10);
        list.push_front(20);
        list.push_back(30);

        assert_eq!(list.len(), 3);
        assert_eq!(list.pop_front(), Some(20));
        assert_eq!(list.pop_back(), Some(30));
        assert_eq!(list.pop_front(), Some(10));
        assert!(list.is_empty());
    }

    #[test]
    fn iterator_test() {
        let mut list = LinkedList::new();
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        let collected: Vec<_> = list.iter().copied().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn remove_test() {
        let mut list = LinkedList::new();
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

        assert!(list.remove(&2));
        assert_eq!(list.len(), 2);

        let collected: Vec<_> = list.iter().copied().collect();
        assert_eq!(collected, vec![1, 3]);
    }
}

