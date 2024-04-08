// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Utilities for rendering lists

use std::mem::MaybeUninit;
use std::pin::Pin;

use web_sys::Node;

use crate::dom::{Anchor, Fragment, FragmentBuilder};
use crate::internal::{In, Out};
use crate::{Mountable, View};

mod keyed;
// mod page_list;

// use page_list::PageList;

pub use keyed::{with, Keyed};

/// Wrapper type that implements `View` for iterators, created by the
/// [`for`](crate::keywords::for) keyword.
#[repr(transparent)]
pub struct List<T>(pub(crate) T);

pub struct ListProduct<P: Mountable> {
    list: Vec<Box<P>>,
    mounted: usize,
    fragment: FragmentBuilder,
}

impl<P> Anchor for ListProduct<P>
where
    P: Mountable,
{
    type Js = Node;
    type Target = Fragment;

    fn anchor(&self) -> &Fragment {
        &self.fragment
    }
}

fn uninit<T>() -> Pin<Box<MaybeUninit<T>>> {
    unsafe {
        let ptr = std::alloc::alloc(std::alloc::Layout::new::<T>());

        Pin::new_unchecked(Box::from_raw(ptr as *mut MaybeUninit<T>))
    }
}

unsafe fn unpin_assume_init<T>(pin: Pin<Box<MaybeUninit<T>>>) -> Box<T> {
    std::mem::transmute(pin)
}

impl<T> View for List<T>
where
    T: IntoIterator,
    <T as IntoIterator>::Item: View,
{
    type Product = ListProduct<<T::Item as View>::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        let iter = self.0.into_iter();
        let fragment = FragmentBuilder::new();

        let list: Vec<_> = iter
            .map(|view| {
                let mut pin = uninit();

                let built = In::pinned(pin.as_mut(), |b| view.build(b));

                fragment.append(built.js());

                unsafe { unpin_assume_init(pin) }
            })
            .collect();

        let mounted = list.len();

        p.put(ListProduct {
            list,
            mounted,
            fragment,
        })
    }

    fn update(self, p: &mut Self::Product) {
        // `mounted` is always within the bounds of `len`, this
        // convinces the compiler that this is indeed the fact,
        // so it can optimize bounds checks here.
        if p.mounted > p.list.len() {
            unsafe { std::hint::unreachable_unchecked() }
        }

        let mut new = self.0.into_iter();
        let mut consumed = 0;

        while let Some(old) = p.list.get_mut(consumed) {
            let Some(new) = new.next() else {
                break;
            };

            new.update(old);
            consumed += 1;
        }

        if consumed < p.mounted {
            for tail in p.list[consumed..p.mounted].iter() {
                tail.unmount();
            }
            p.mounted = consumed;
            return;
        }

        p.list.extend(new.map(|view| {
            let mut pin = uninit();

            In::pinned(pin.as_mut(), |b| view.build(b));

            consumed += 1;

            unsafe { unpin_assume_init(pin) }
        }));

        for built in p.list[p.mounted..consumed].iter() {
            p.fragment.append(built.js());
        }

        p.mounted = consumed;
    }
}

impl<V: View> View for Vec<V> {
    type Product = ListProduct<V::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        List(self).build(p)
    }

    fn update(self, p: &mut Self::Product) {
        List(self).update(p);
    }
}

impl<'a, V> View for &'a [V]
where
    &'a V: View,
{
    type Product = ListProduct<<&'a V as View>::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        List(self).build(p)
    }

    fn update(self, p: &mut Self::Product) {
        List(self).update(p)
    }
}

impl<V: View, const N: usize> View for [V; N] {
    type Product = ListProduct<V::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        List(self).build(p)
    }

    fn update(self, p: &mut Self::Product) {
        List(self).update(p)
    }
}
