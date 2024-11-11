use std::ptr::NonNull;
use std::{marker::PhantomData, mem::replace};

use crate::internal::{init, In, Out};

struct Node<P> {
    item: P,
    next: Next<P>,
}

type Next<P> = Option<Box<Node<P>>>;

pub struct LinkedList<P> {
    head: Option<Box<Node<P>>>,
    tail: NonNull<Next<P>>,
}

impl<P> LinkedList<P> {
    pub fn build(p: In<Self>) -> Out<Self> {
        p.in_place(|p| unsafe {
            let head = &mut *init!(p.head = None);

            init!(p.tail = NonNull::from(&*head).cast());

            Out::from_raw(p)
        })
    }

    pub fn push<F>(&mut self, constructor: F)
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

        unsafe {
            // Assign the node to old tail
            *tail.as_ptr() = Some(node);
        }
    }

    pub fn iter(&mut self) -> ListIter<P> {
        ListIter {
            next: NonNull::from(&mut self.head),
            _lt: PhantomData,
        }
    }
}

pub struct ListIter<'a, P> {
    next: NonNull<Option<Box<Node<P>>>>,
    _lt: PhantomData<&'a ()>,
}

impl<'a, P> ListIter<'a, P> {
    pub fn peek(&mut self) -> Option<&mut P> {
        match unsafe { self.next.as_mut() } {
            Some(ref mut node) => Some(&mut node.item),
            None => None,
        }
    }
}

impl<'a, P> Iterator for ListIter<'a, P>
where
    P: 'a,
{
    type Item = &'a mut P;

    fn next(&mut self) -> Option<Self::Item> {
        match unsafe { self.next.as_mut() } {
            Some(ref mut node) => {
                self.next = NonNull::from(&mut node.next);

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
        let mut list = In::boxed(LinkedList::<u32>::build);

        list.push(|n| n.put(42));
        list.push(|n| n.put(1337));

        assert_eq!(&[42, 1337][..], list.iter().map(|n| *n).collect::<Vec<_>>());

        list.push(|n| n.put(0xDEADBEEF));

        assert_eq!(
            &[42, 1337, 0xDEADBEEF][..],
            list.iter().map(|n| *n).collect::<Vec<_>>()
        );
    }
}
