use std::mem::replace;
use std::ptr::NonNull;

use crate::internal::{init, In, Out};

struct Node<P> {
    item: P,
    next: Next<P>,
}

type Next<P> = Option<Box<Node<P>>>;

impl<P> Node<P> {
    fn new(item: P) -> Box<Self> {
        Box::new(Node { item, next: None })
    }
}

pub struct List<P> {
    head: Option<Box<Node<P>>>,
    tail: NonNull<Next<P>>,
}

impl<P> List<P> {
    pub fn build(p: In<Self>) -> Out<Self> {
        p.in_place(|p| unsafe {
            let head = &mut *init!(p.head = None);

            init!(p.tail = NonNull::from(&*head).cast());

            Out::from_raw(p)
        })
    }

    pub fn push<F>(&mut self, constructor: F) -> &mut P
    where
        F: FnOnce(In<P>) -> Out<P>,
    {
        let node = In::boxed(|node: In<Node<P>>| {
            node.in_place(move |p| unsafe {
                init!(p.item @ constructor(p));
                init!(p.next = None);

                Out::from_raw(p)
            })
        });

        // Update the tail to the `next` field of the newly created
        // `Node`, get the old pointer.
        let tail = replace(&mut self.tail, NonNull::from(&node.next));
        let ret = NonNull::from(&node.item);

        unsafe {
            // Assign the node to old tail
            *tail.as_ptr() = Some(node);

            &mut *ret.as_ptr()
        }
    }

    pub fn iter(&mut self) -> ListIter<P> {
        ListIter {
            next: self.head.as_deref_mut(),
        }
    }
}

pub struct ListIter<'a, P> {
    next: Option<&'a mut Node<P>>,
}

impl<'a, P> Iterator for ListIter<'a, P> {
    type Item = &'a mut P;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next.take() {
            Some(node) => {
                self.next = node.next.as_deref_mut();

                Some(&mut node.item)
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let mut list = In::boxed(List::<u32>::build);

        list.push(|n| n.put(42));
        list.push(|n| n.put(1337));

        assert_eq!(&[42, 1337][..], list.iter().map(|n| *n).collect::<Vec<_>>());

        list.push(|n| n.put(0xDEADBEEF));

        assert_eq!(&[42, 1337, 0xDEADBEEF][..], list.iter().map(|n| *n).collect::<Vec<_>>());
    }
}
