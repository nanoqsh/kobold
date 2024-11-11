use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use crate::internal::{init, In, Out};

struct Node<P> {
    item: P,
    next: Next<Node<P>>,
}

type Next<T> = MaybeUninit<NonNull<T>>;

pub struct LinkedList<P> {
    head: Next<Node<P>>,
    tail: NonNull<Next<Node<P>>>,
    len: usize,
}

impl<P> Drop for LinkedList<P> {
    fn drop(&mut self) {
        for _ in 0..self.len {
            let node = unsafe { Box::from_raw(self.head.assume_init_read().as_ptr()) };

            self.head = node.next;

            drop(node.item);
        }
    }
}

impl<P> LinkedList<P> {
    pub fn build(p: In<Self>) -> Out<Self> {
        p.in_place(|p| unsafe {
            let tail = NonNull::new_unchecked(&raw mut (*p).head);

            init!(p.tail = tail);
            init!(p.len = 0);

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

                Out::from_raw(p)
            })
        });

        let next = NonNull::from(&node.next);
        unsafe { self.tail.as_mut().write(Box::leak(node).into()) };

        self.tail = next;
        self.len += 1;
    }

    pub fn iter(&mut self) -> ListIter<P> {
        ListIter {
            pos: 0,
            end: self.len,
            next: self.head,
            _lt: PhantomData,
        }
    }
}

pub struct ListIter<'a, P> {
    pos: usize,
    end: usize,
    next: Next<Node<P>>,
    _lt: PhantomData<&'a ()>,
}

impl<P> ListIter<'_, P> {
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn limit(&mut self, limit: usize) {
        self.end = std::cmp::min(self.end, limit);
    }
}

impl<'a, P> Iterator for ListIter<'a, P>
where
    P: 'a,
{
    type Item = &'a mut P;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }

        let node = unsafe { self.next.assume_init_mut().as_mut() };

        self.next = node.next;
        self.pos += 1;

        Some(&mut node.item)
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
