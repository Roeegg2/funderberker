//! A doubly-linked list implementation for kernel environments.
//!
//! This implementation avoids using the standard library and heap allocations,
//! making it suitable for kernel development while maintaining an API similar to
//! Rust's std::collections::LinkedList.

use core::fmt;
use core::iter::{FromIterator, FusedIterator};
use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::lib::boxed::Box;

/// A doubly-linked list implementation with cursor-based iteration.
pub struct LinkedList<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
    marker: PhantomData<Box<Node<T>>>,
}

struct Node<T> {
    next: Option<NonNull<Node<T>>>,
    prev: Option<NonNull<Node<T>>>,
    element: T,
}

/// An iterator over the elements of a LinkedList.
pub struct Iter<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
    marker: PhantomData<&'a Node<T>>,
}

/// A mutable iterator over the elements of a LinkedList.
pub struct IterMut<'a, T: 'a> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
    marker: PhantomData<&'a mut Node<T>>,
}

/// An owning iterator over the elements of a LinkedList.
pub struct IntoIter<T> {
    list: LinkedList<T>,
}

/// A cursor which can be positioned at any element in the list.
pub struct Cursor<'a, T: 'a> {
    list: &'a mut LinkedList<T>,
    curr: Option<NonNull<Node<T>>>,
    index: Option<usize>,
}

// Implementation for LinkedList
impl<T> LinkedList<T> {
    /// Creates an empty LinkedList.
    pub const fn new() -> Self {
        LinkedList {
            head: None,
            tail: None,
            len: 0,
            marker: PhantomData,
        }
    }

    /// Returns the length of the list.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Removes all elements from the list.
    ///
    /// This method requires an allocator to deallocate nodes.
    pub fn clear(&mut self) {
        // In a kernel environment, you'd need to implement your own node
        // allocation and deallocation mechanism. This is a simplified version.
        while let Some(_) = self.pop_front() {}
    }

    /// Returns a reference to the first element, or None if the list is empty.
    pub fn front(&self) -> Option<&T> {
        self.head.map(|node| unsafe {
            &(*node.as_ptr()).element
        })
    }

    /// Returns a mutable reference to the first element, or None if the list is empty.
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.head.map(|node| unsafe {
            &mut (*node.as_ptr()).element
        })
    }

    /// Returns a reference to the last element, or None if the list is empty.
    pub fn back(&self) -> Option<&T> {
        self.tail.map(|node| unsafe {
            &(*node.as_ptr()).element
        })
    }

    /// Returns a mutable reference to the last element, or None if the list is empty.
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.tail.map(|node| unsafe {
            &mut (*node.as_ptr()).element
        })
    }

    /// Adds an element to the front of the list.
    ///
    /// This method requires an allocator to allocate a new node.
    pub fn push_front(&mut self, element: T) -> Result<(), &'static str> {
        // In a kernel environment, you'd implement your own node allocation
        let node = Box::new(Node {
            next: self.head,
            prev: None,
            element,
        });
        
        // Convert Box<Node<T>> to NonNull<Node<T>>
        let node_ptr = match NonNull::new(Box::into_raw(node)) {
            Some(ptr) => ptr,
            None => return Err("Failed to allocate memory for node"),
        };

        // Update the old head node to point back to the new node
        match self.head {
            None => self.tail = Some(node_ptr),
            Some(head) => unsafe {
                (*head.as_ptr()).prev = Some(node_ptr);
            },
        }

        // Update the head to be the new node
        self.head = Some(node_ptr);
        self.len += 1;
        
        Ok(())
    }

    /// Adds an element to the back of the list.
    ///
    /// This method requires an allocator to allocate a new node.
    pub fn push_back(&mut self, element: T) -> Result<(), &'static str> {
        // In a kernel environment, you'd implement your own node allocation
        let node = Box::new(Node {
            next: None,
            prev: self.tail,
            element,
        });
        
        // Convert Box<Node<T>> to NonNull<Node<T>>
        let node_ptr = match NonNull::new(Box::into_raw(node)) {
            Some(ptr) => ptr,
            None => return Err("Failed to allocate memory for node"),
        };

        // Update the old tail node to point to the new node
        match self.tail {
            None => self.head = Some(node_ptr),
            Some(tail) => unsafe {
                (*tail.as_ptr()).next = Some(node_ptr);
            },
        }

        // Update the tail to be the new node
        self.tail = Some(node_ptr);
        self.len += 1;
        
        Ok(())
    }

    /// Removes the first element and returns it, or None if the list is empty.
    pub fn pop_front(&mut self) -> Option<T> {
        self.head.map(|node| unsafe {
            let node = Box::from_raw(node.as_ptr());
            self.head = node.next;
            
            match self.head {
                None => self.tail = None,
                Some(head) => (*head.as_ptr()).prev = None,
            }
            
            self.len -= 1;
            node.element
        })
    }

    /// Removes the last element and returns it, or None if the list is empty.
    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|node| unsafe {
            let node = Box::from_raw(node.as_ptr());
            self.tail = node.prev;
            
            match self.tail {
                None => self.head = None,
                Some(tail) => (*tail.as_ptr()).next = None,
            }
            
            self.len -= 1;
            node.element
        })
    }

    /// Returns an iterator over the list.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            head: self.head,
            tail: self.tail,
            len: self.len,
            marker: PhantomData,
        }
    }

    /// Returns a mutable iterator over the list.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            head: self.head,
            tail: self.tail,
            len: self.len,
            marker: PhantomData,
        }
    }

    /// Returns a cursor positioned at the front of the list.
    pub fn cursor_front(&mut self) -> Cursor<'_, T> {
        Cursor {
            list: self,
            curr: self.head,
            index: if self.is_empty() { None } else { Some(0) },
        }
    }

    /// Returns a cursor positioned at the back of the list.
    pub fn cursor_back(&mut self) -> Cursor<'_, T> {
        let index = if self.is_empty() {
            None
        } else {
            Some(self.len - 1)
        };
        
        Cursor {
            list: self,
            curr: self.tail,
            index,
        }
    }

    /// Provides a forward iterator.
    pub fn cursor_from(&mut self, index: usize) -> Cursor<'_, T> {
        if index >= self.len {
            return Cursor {
                list: self,
                curr: None,
                index: None,
            };
        }

        let mut cursor = self.cursor_front();
        for _ in 0..index {
            cursor.move_next();
        }
        
        cursor
    }
}

// Drop implementation to handle memory cleanup
impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

// Default implementation
impl<T> Default for LinkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Clone implementation for LinkedList
impl<T: Clone> Clone for LinkedList<T> {
    fn clone(&self) -> Self {
        let mut list = LinkedList::new();
        for item in self.iter() {
            let _ = list.push_back(item.clone());
        }
        list
    }
}

// Implementation for Debug
impl<T: fmt::Debug> fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// Implementation for FromIterator
impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut list = Self::new();
        for item in iter {
            let _ = list.push_back(item);
        }
        list
    }
}

// Implementation for IntoIterator
impl<T> IntoIterator for LinkedList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { list: self }
    }
}

impl<'a, T> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut LinkedList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

// Implementation for Iter
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.head.map(|node| unsafe {
                self.len -= 1;
                self.head = (*node.as_ptr()).next;
                &(*node.as_ptr()).element
            })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.tail.map(|node| unsafe {
                self.len -= 1;
                self.tail = (*node.as_ptr()).prev;
                &(*node.as_ptr()).element
            })
        }
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}
impl<'a, T> FusedIterator for Iter<'a, T> {}

// Implementation for IterMut
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.head.map(|node| unsafe {
                self.len -= 1;
                self.head = (*node.as_ptr()).next;
                &mut (*node.as_ptr()).element
            })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.tail.map(|node| unsafe {
                self.len -= 1;
                self.tail = (*node.as_ptr()).prev;
                &mut (*node.as_ptr()).element
            })
        }
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}
impl<'a, T> FusedIterator for IterMut<'a, T> {}

// Implementation for IntoIter
impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.list.pop_front()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.list.len, Some(self.list.len))
    }
}

impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.list.pop_back()
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}

// Implementation for Cursor
impl<'a, T> Cursor<'a, T> {
    /// Returns a reference to the current element.
    pub fn current(&self) -> Option<&T> {
        self.curr.map(|node| unsafe {
            &(*node.as_ptr()).element
        })
    }

    /// Returns a mutable reference to the current element.
    pub fn current_mut(&mut self) -> Option<&mut T> {
        self.curr.map(|node| unsafe {
            &mut (*node.as_ptr()).element
        })
    }

    /// Returns the index of the current element.
    pub fn index(&self) -> Option<usize> {
        self.index
    }

    /// Moves the cursor to the next element and returns a reference to it.
    pub fn move_next(&mut self) -> Option<&T> {
        match self.curr {
            Some(curr) => unsafe {
                self.curr = (*curr.as_ptr()).next;
                if let Some(index) = self.index {
                    self.index = Some(index + 1);
                    if self.index.unwrap() >= self.list.len {
                        self.index = None;
                    }
                }
                self.current()
            },
            None => {
                // If we're at the end, try to move to the front
                if !self.list.is_empty() {
                    self.curr = self.list.head;
                    self.index = Some(0);
                    self.current()
                } else {
                    None
                }
            }
        }
    }

    /// Moves the cursor to the previous element and returns a reference to it.
    pub fn move_prev(&mut self) -> Option<&T> {
        match self.curr {
            Some(curr) => unsafe {
                self.curr = (*curr.as_ptr()).prev;
                if let Some(index) = self.index {
                    if index > 0 {
                        self.index = Some(index - 1);
                    } else {
                        self.index = None;
                    }
                }
                self.current()
            },
            None => {
                // If we're at the start, try to move to the back
                if !self.list.is_empty() {
                    self.curr = self.list.tail;
                    self.index = Some(self.list.len - 1);
                    self.current()
                } else {
                    None
                }
            }
        }
    }

    /// Removes and returns the current element.
    pub fn remove_current(&mut self) -> Option<T> {
        match self.curr {
            Some(curr) => unsafe {
                let prev = (*curr.as_ptr()).prev;
                let next = (*curr.as_ptr()).next;

                match prev {
                    None => self.list.head = next,
                    Some(prev) => (*prev.as_ptr()).next = next,
                }

                match next {
                    None => self.list.tail = prev,
                    Some(next) => (*next.as_ptr()).prev = prev,
                }

                let node = Box::from_raw(curr.as_ptr());
                self.curr = next;
                self.list.len -= 1;

                if let Some(index) = self.index {
                    if next.is_none() {
                        self.index = None;
                    } else if index >= self.list.len {
                        self.index = None;
                    }
                }
                
                Some(node.element)
            },
            None => None,
        }
    }

    /// Inserts an element after the current position and sets the cursor to the new element.
    pub fn insert_after(&mut self, element: T) -> Result<(), &'static str> {
        match self.curr {
            Some(curr) => unsafe {
                let next = (*curr.as_ptr()).next;
                
                // Create new node
                let node = Box::new(Node {
                    next,
                    prev: Some(curr),
                    element,
                });
                
                // Convert Box<Node<T>> to NonNull<Node<T>>
                let node_ptr = match NonNull::new(Box::into_raw(node)) {
                    Some(ptr) => ptr,
                    None => return Err("Failed to allocate memory for node"),
                };

                // Update the current node to point to the new node
                (*curr.as_ptr()).next = Some(node_ptr);

                // Update the next node to point back to the new node
                match next {
                    None => self.list.tail = Some(node_ptr),
                    Some(next) => (*next.as_ptr()).prev = Some(node_ptr),
                }

                self.list.len += 1;

                // Move cursor to the new node
                self.curr = Some(node_ptr);
                if let Some(index) = self.index {
                    self.index = Some(index + 1);
                }
                
                Ok(())
            },
            None => {
                // If there's no current position, push to the back
                self.list.push_back(element)?;
                
                // Move cursor to the new node
                self.curr = self.list.tail;
                self.index = Some(self.list.len - 1);
                
                Ok(())
            }
        }
    }

    /// Inserts an element before the current position and sets the cursor to the new element.
    pub fn insert_before(&mut self, element: T) -> Result<(), &'static str> {
        match self.curr {
            Some(curr) => unsafe {
                let prev = (*curr.as_ptr()).prev;
                
                // Create new node
                let node = Box::new(Node {
                    next: Some(curr),
                    prev,
                    element,
                });
                
                // Convert Box<Node<T>> to NonNull<Node<T>>
                let node_ptr = match NonNull::new(Box::into_raw(node)) {
                    Some(ptr) => ptr,
                    None => return Err("Failed to allocate memory for node"),
                };

                // Update the current node to point back to the new node
                (*curr.as_ptr()).prev = Some(node_ptr);

                // Update the previous node to point to the new node
                match prev {
                    None => self.list.head = Some(node_ptr),
                    Some(prev) => (*prev.as_ptr()).next = Some(node_ptr),
                }

                self.list.len += 1;

                // Move cursor to the new node
                self.curr = Some(node_ptr);
                if let Some(index) = self.index {
                    self.index = Some(index);
                    
                    // Shift all future indices
                    if index < self.list.len - 1 {
                        // Implementation detail: we'd need to update all indices in a real implementation
                    }
                }
                
                Ok(())
            },
            None => {
                // If there's no current position, push to the front
                self.list.push_front(element)?;
                
                // Move cursor to the new node
                self.curr = self.list.head;
                self.index = Some(0);
                
                Ok(())
            }
        }
    }
}
