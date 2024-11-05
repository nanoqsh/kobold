// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::{self, Debug, Display, Write};
use std::hash::{Hash, Hasher};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

use crate::diff::Diff;

pub struct Ver<T> {
    inner: VerValue<T>,
    ver: usize,
}

impl<T> Ver<T> {
    pub fn new<U>(val: U) -> Self
    where
        U: Into<T>,
    {
        Ver {
            inner: VerValue {
                val: ManuallyDrop::new(val.into()),
            },
            ver: 0,
        }
    }

    pub fn into_inner(self) -> T {
        let mut this = ManuallyDrop::new(self);

        unsafe { ManuallyDrop::take(&mut this.inner.val) }
    }
}

union VerValue<T> {
    val: ManuallyDrop<T>,
    noise: usize,
}

impl<T> VerValue<T> {
    fn noise(&self) -> u64 {
        if align_of::<T>() == align_of::<usize>() && (size_of::<T>() % size_of::<usize>() == 0) {
            unsafe { self.noise as u64 }
        } else {
            0
        }
    }

    fn val(&self) -> &T {
        unsafe { &self.val }
    }

    fn val_mut(&mut self) -> &mut T {
        unsafe { &mut self.val }
    }
}

impl<T> Drop for Ver<T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.inner.val) }
    }
}

impl<T> Diff for &'_ Ver<T> {
    type Memo = u64;

    fn into_memo(self) -> Self::Memo {
        (self.ver as u64).wrapping_shl(32) | self.inner.noise()
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
        self.inner.val()
    }
}

impl<T> DerefMut for Ver<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ver += 1;

        self.inner.val_mut()
    }
}

impl<T> PartialEq<T> for Ver<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        self.inner.val().eq(other)
    }
}

impl<T> PartialEq for Ver<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Ver<T>) -> bool {
        self.inner.val().eq(other.inner.val())
    }
}

impl<T> Eq for Ver<T> where T: Eq {}

impl<T> PartialOrd<Ver<T>> for Ver<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Ver<T>) -> Option<std::cmp::Ordering> {
        self.inner.val().partial_cmp(other.inner.val())
    }
}

impl<T> Ord for Ver<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.val().cmp(other.inner.val())
    }
}

impl<T> Debug for Ver<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self.inner.val(), f)
    }
}

impl<T> Display for Ver<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(self.inner.val(), f)
    }
}

impl<T> Write for Ver<T>
where
    T: Write,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.ver += 1;
        self.inner.val_mut().write_str(s)
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
        self.inner.val().hash(state)
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
