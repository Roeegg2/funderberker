use core::ptr::NonNull;
use core::marker::PhantomData;
use core::fmt;
use alloc::boxed::Box;

pub struct LinkedList<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
    _marker: PhantomData<Box<Node<T>>>,
}

pub struct Node<T> {
    pub data: T,
    next: Option<NonNull<Node<T>>>,
    prev: Option<NonNull<Node<T>>>,
}

impl<T> Node<T> {
    pub fn new(data: T) -> Self {
        Node { data, next: None, prev: None }
    }
    pub fn data(&self) -> &T { &self.data }
    pub fn element_mut(&mut self) -> &mut T { &mut self.data }
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

    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }

    pub fn clear(&mut self) {
        while self.pop_front().is_some() {}
    }

    pub fn front(&self) -> Option<&T> {
        self.head.map(|node| unsafe { &node.as_ref().data })
    }
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.head.map(|node| unsafe { &mut node.as_ptr().as_mut().unwrap().data })
    }
    pub fn back(&self) -> Option<&T> {
        self.tail.map(|node| unsafe { &node.as_ref().data })
    }
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.tail.map(|node| unsafe { &mut node.as_ptr().as_mut().unwrap().data })
    }

    pub fn push_front(&mut self, data: T) {
        let mut new_node = Box::new(Node::new(data));
        new_node.next = self.head;
        new_node.prev = None;
        let new_node = Box::into_raw(new_node);
        let new_node = unsafe { NonNull::new_unchecked(new_node) };
        unsafe {
            if let Some(mut old_head) = self.head {
                old_head.as_mut().prev = Some(new_node);
            } else {
                self.tail = Some(new_node);
            }
        }
        self.head = Some(new_node);
        self.len += 1;
    }

    pub fn push_back(&mut self, data: T) {
        let mut new_node = Box::new(Node::new(data));
        new_node.next = None;
        new_node.prev = self.tail;
        let new_node = Box::into_raw(new_node);
        let new_node = unsafe { NonNull::new_unchecked(new_node) };
        unsafe {
            if let Some(mut old_tail) = self.tail {
                old_tail.as_mut().next = Some(new_node);
            } else {
                self.head = Some(new_node);
            }
        }
        self.tail = Some(new_node);
        self.len += 1;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.head.map(|node| unsafe {
            let mut boxed = Box::from_raw(node.as_ptr());
            self.head = boxed.next;
            if let Some(mut new_head) = self.head {
                new_head.as_mut().prev = None;
            } else {
                self.tail = None;
            }
            self.len -= 1;
            boxed.next = None; boxed.prev = None;
            boxed.data
        })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|node| unsafe {
            let mut boxed = Box::from_raw(node.as_ptr());
            self.tail = boxed.prev;
            if let Some(mut new_tail) = self.tail {
                new_tail.as_mut().next = None;
            } else {
                self.head = None;
            }
            self.len -= 1;
            boxed.next = None; boxed.prev = None;
            boxed.data
        })
    }

    pub fn create_node(data: T) -> Box<Node<T>> {
        Box::new(Node::new(data))
    }

    /// Pushes a boxed node to the front. The node must not be part of another list.
    pub fn push_node_front(&mut self, mut node: Box<Node<T>>) {
        node.next = self.head;
        node.prev = None;
        let node_ptr = Box::into_raw(node);
        let node_ptr = unsafe { NonNull::new_unchecked(node_ptr) };
        unsafe {
            if let Some(mut old_head) = self.head {
                old_head.as_mut().prev = Some(node_ptr);
            } else {
                self.tail = Some(node_ptr);
            }
        }
        self.head = Some(node_ptr);
        self.len += 1;
    }

    /// Pushes a boxed node to the back. The node must not be part of another list.
    pub fn push_node_back(&mut self, mut node: Box<Node<T>>) {
        node.prev = self.tail;
        node.next = None;
        let node_ptr = Box::into_raw(node);
        let node_ptr = unsafe { NonNull::new_unchecked(node_ptr) };
        unsafe {
            if let Some(mut old_tail) = self.tail {
                old_tail.as_mut().next = Some(node_ptr);
            } else {
                self.head = Some(node_ptr);
            }
        }
        self.tail = Some(node_ptr);
        self.len += 1;
    }

    /// Pops the front node and returns it as Box<Node<T>>.
    pub fn pop_node_front(&mut self) -> Option<Box<Node<T>>> {
        self.head.map(|node| unsafe {
            let mut boxed = Box::from_raw(node.as_ptr());
            self.head = boxed.next;
            if let Some(mut new_head) = self.head {
                new_head.as_mut().prev = None;
            } else {
                self.tail = None;
            }
            self.len -= 1;
            boxed.next = None;
            boxed.prev = None;
            boxed
        })
    }

    /// Pops the back node and returns it as Box<Node<T>>.
    pub fn pop_node_back(&mut self) -> Option<Box<Node<T>>> {
        self.tail.map(|node| unsafe {
            let mut boxed = Box::from_raw(node.as_ptr());
            self.tail = boxed.prev;
            if let Some(mut new_tail) = self.tail {
                new_tail.as_mut().next = None;
            } else {
                self.head = None;
            }
            self.len -= 1;
            boxed.next = None;
            boxed.prev = None;
            boxed
        })
    }

    // Remove node at the given index, returning it as Box<Node<T>>
    pub fn remove_at_node(&mut self, index: usize) -> Option<Box<Node<T>>> {
        if index >= self.len { return None; }
        let mut curr = self.head?;
        for _ in 0..index {
            curr = unsafe { curr.as_ref().next? };
        }
        unsafe {
            // Update previous node's next
            if let Some(mut prev) = curr.as_ref().prev {
                prev.as_mut().next = curr.as_ref().next;
            } else {
                self.head = curr.as_ref().next;
            }
            // Update next node's prev
            if let Some(mut next) = curr.as_ref().next {
                next.as_mut().prev = curr.as_ref().prev;
            } else {
                self.tail = curr.as_ref().prev;
            }
            self.len -= 1;
            let mut boxed = Box::from_raw(curr.as_ptr());
            boxed.next = None;
            boxed.prev = None;
            Some(boxed)
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }
}

// Iterator that borrows the list for its lifetime, so mutation is impossible
pub struct Iter<'a, T> {
    next: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let node = self.next?;
        unsafe {
            self.next = node.as_ref().next;
            self.remaining -= 1;
            Some(&node.as_ref().data)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<'a, T> ExactSizeIterator for Iter<'a, T> {}

pub struct IterMut<'a, T> {
    next: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut node = self.next?;
        unsafe {
            self.next = node.as_ref().next;
            self.remaining -= 1;
            Some(&mut node.as_mut().data)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}

impl<T> LinkedList<T> {
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            next: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }
}

pub struct IterMutNode<'a, T> {
    next: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a mut Node<T>>,
}

impl<'a, T> Iterator for IterMutNode<'a, T> {
    type Item = &'a mut Node<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut node = self.next?;
        unsafe {
            self.next = node.as_ref().next;
            self.remaining -= 1;
            Some(node.as_mut())
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for IterMutNode<'a, T> {}

impl<T> LinkedList<T> {
    pub fn iter_mut_node(&mut self) -> IterMutNode<'_, T> {
        IterMutNode {
            next: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }
}

pub struct IterNode<'a, T> {
    next: Option<NonNull<Node<T>>>,
    remaining: usize,
    _marker: PhantomData<&'a Node<T>>,
}

impl<'a, T> Iterator for IterNode<'a, T> {
    type Item = &'a Node<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let node = self.next?;
        unsafe {
            self.next = node.as_ref().next;
            self.remaining -= 1;
            Some(node.as_ref())
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, T> ExactSizeIterator for IterNode<'a, T> {}

impl<T> LinkedList<T> {
    pub fn iter_nodes(&self) -> IterNode<'_, T> {
        IterNode {
            next: self.head,
            remaining: self.len,
            _marker: PhantomData,
        }
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T: fmt::Debug> fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: fmt::Debug> fmt::Debug for Node<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node").field("data", &self.data).finish()
    }
}

impl<T> Default for LinkedList<T> {
    fn default() -> Self { Self::new() }
}
