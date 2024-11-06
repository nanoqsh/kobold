// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::fmt::{self, Debug, Display, Write};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

use crate::diff::{Diff, Fence};
use crate::View;

#[inline]
fn next_ver() -> usize {
    thread_local! {
        static VER: Cell<usize> = const { Cell::new(0) };
    }

    let ver = VER.get();

    VER.set(ver.wrapping_add(1));

    ver
}

pub struct Ver<T> {
    inner: T,
    ver: usize,
}

impl<T> Ver<T> {
    pub fn new<U>(val: U) -> Self
    where
        U: Into<T>,
    {
        Ver {
            inner: val.into(),
            ver: next_ver(),
        }
    }

    pub const fn fence<V, F>(&self, render: F) -> Fence<usize, F>
    where
        V: View,
        F: FnOnce() -> V,
    {
        Fence {
            guard: self.ver,
            inner: render,
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Diff for &'_ Ver<T> {
    type Memo = usize;

    fn into_memo(self) -> Self::Memo {
        self.ver
    }

    fn diff(self, memo: &mut Self::Memo) -> bool {
        let m = self.into_memo();

        if *memo != m {
            *memo = m;
            true
        } else {
            false
        }
    }
}

impl<T> Deref for Ver<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Ver<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ver = next_ver();

        &mut self.inner
    }
}

impl<T, U> PartialEq<U> for Ver<T>
where
    T: PartialEq<U>,
{
    fn eq(&self, other: &U) -> bool {
        self.inner.eq(other)
    }
}

impl<T> Debug for Ver<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl<T> Display for Ver<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl<T> Write for Ver<T>
where
    T: Write,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.ver = next_ver();
        self.inner.write_str(s)
    }
}

impl<T, A> FromIterator<A> for Ver<T>
where
    T: FromIterator<A>,
{
    fn from_iter<I>(iter: I) -> Ver<T>
    where
        I: IntoIterator<Item = A>,
    {
        Ver::new(T::from_iter(iter))
    }
}

impl<T> Hash for Ver<T>
where
    T: Hash,
{
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.inner.hash(state)
    }
}

#[cfg(feature = "serde")]
mod serde {
    use serde::de::{Deserialize, Deserializer};
    use serde::ser::{Serialize, Serializer};

    use super::Ver;

    impl<T> Serialize for Ver<T>
    where
        T: Serialize,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            self.inner.val().serialize(serializer)
        }
    }

    impl<'de, T> Deserialize<'de> for Ver<T>
    where
        T: Deserialize<'de>,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            T::deserialize(deserializer).map(Ver::new)
        }
    }
}
