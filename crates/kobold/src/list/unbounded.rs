// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Utilities for rendering lists

use web_sys::Node;

use crate::dom::{Anchor, Fragment, FragmentBuilder};
use crate::internal::{init, In, Out};
use crate::{Mountable, View};

mod list;

use list::LinkedList;

pub struct ListProduct<P: Mountable> {
    list: LinkedList<P>,
    mounted: usize,
    fragment: FragmentBuilder,
}

impl<P: Mountable> ListProduct<P> {
    pub fn build<I>(iter: I, p: In<Self>) -> Out<Self>
    where
        I: Iterator,
        I::Item: View<Product = P>,
    {
        let mut list = p.in_place(|p| unsafe {
            init!(p.list @ LinkedList::build(p));
            init!(p.mounted = 0);
            init!(p.fragment = FragmentBuilder::new());

            Out::from_raw(p)
        });

        list.extend(iter);
        list
    }

    pub fn update<I>(&mut self, mut iter: I)
    where
        I: Iterator,
        I::Item: View<Product = P>,
    {
        let mut list = self.list.iter();
        let mut updated = 0;

        while let Some(old) = list.peek() {
            let Some(new) = iter.next() else {
                self.mounted = updated;

                for old in list {
                    old.unmount();
                }

                return;
            };

            new.update(old);

            updated += 1;

            if updated > self.mounted {
                self.fragment.append(old.js());
            }

            list.next();
        }

        self.extend(iter);
    }

    fn extend<I>(&mut self, iter: I)
    where
        I: Iterator,
        I::Item: View<Product = P>,
    {
        for view in iter {
            self.list.push(|p| {
                let built = view.build(p);

                self.fragment.append(built.js());

                built
            });
        }
    }
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
