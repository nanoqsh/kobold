// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Utilities for diffing values in render functions.

use std::ops::Deref;
use std::ptr::NonNull;

use web_sys::Node;

use crate::attribute::Attribute;
use crate::dom::{Anchor, TextContent};
use crate::internal::{In, Out};
use crate::value::{IntoText, Value};
use crate::{init, Mountable, View};

mod ver;

pub use ver::Ver;

/// Create a wrapper around a `view` that will prevent updates to it, unless
/// the value of `guard` has changed.
///
/// Fencing against updates can be a great optimization that combines well
/// with the [`use`](crate::keywords::use) keyword.
///
/// ```
/// use kobold::prelude::*;
/// use kobold::diff::fence;
///
/// struct User {
///     id: usize,
///     name: String,
///     email: String,
/// }
///
/// #[component]
/// fn user_row(user: &User) -> impl View + '_ {
///     fence(user.id, || view! {
///         // This row is only re-rendered if `user.id` has changed
///         <tr>
///             <td>{ user.id }</td>
///
///             // Assuming that `name` never changes for a `User`
///             // we can disable diffing here with the `use` keyword.
///             <td>{ use &user.name }</td>
///             <td>{ use &user.email }</td>
///         </tr>
///     })
/// }
/// # fn main() {}
/// ```
pub const fn fence<D, V, F>(guard: D, render: F) -> Fence<D, F>
where
    D: Diff,
    V: View,
    F: FnOnce() -> V,
{
    Fence {
        guard,
        inner: render,
    }
}

/// Create a wrapper around a `view` that will prevent updates to it.
///
/// This is effectively an unconditional [`fence`].
///
/// ```
/// use kobold::prelude::*;
/// use kobold::diff::invar;
///
/// #[component]
/// fn tag(label: &'static str) -> impl View {
///     invar(move || view! {
///         <span.tag>{ static label }</span>
///     })
/// }
/// # fn main() {}
/// ```
pub const fn invar<F, V>(render: F) -> Invar<F>
where
    F: FnOnce() -> V,
    V: View,
{
    Invar(render)
}

/// Smart [`View`] that prevents updates, see [`invar`].
#[repr(transparent)]
pub struct Invar<F>(F);

impl<V, F> View for Invar<F>
where
    F: FnOnce() -> V,
    V: View,
{
    type Product = V::Product;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        (self.0)().build(p)
    }

    fn update(self, _: &mut Self::Product) {}
}

/// Smart [`View`] that guards against unnecessary renders, see [`fence`].
pub struct Fence<D, F> {
    guard: D,
    inner: F,
}

impl<D, F, V> View for Fence<D, F>
where
    D: Diff,
    F: FnOnce() -> V,
    V: View,
{
    type Product = Fence<D::Memo, V::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        p.in_place(|p| unsafe {
            init!(p.guard = self.guard.into_memo());
            init!(p.inner @ (self.inner)().build(p));

            Out::from_raw(p)
        })
    }

    fn update(self, p: &mut Self::Product) {
        if self.guard.diff(&mut p.guard) {
            (self.inner)().update(&mut p.inner);
        }
    }
}

impl<D, P> Anchor for Fence<D, P>
where
    P: Mountable,
{
    type Js = P::Js;
    type Target = P;

    fn anchor(&self) -> &P {
        &self.inner
    }
}

/// Trait that defines how different values can be _diffed_ at runtime.
pub trait Diff: Copy {
    /// Data used to check if current value is different from the one in the past.
    type Memo: 'static;

    /// Generate a new `Memo` for this type.
    fn into_memo(self) -> Self::Memo;

    /// Diff current value against the `Memo`, update it if necessary and return
    /// `true` if it has changed.
    fn diff(self, memo: &mut Self::Memo) -> bool;
}

impl<T> Diff for Option<T>
where
    T: Copy + Eq + 'static,
{
    type Memo = Self;

    fn into_memo(self) -> Self {
        self
    }

    fn diff(self, memo: &mut Self) -> bool {
        if self != *memo {
            *memo = self;
            true
        } else {
            false
        }
    }
}

macro_rules! impl_diff_str {
    ($($ty:ty),*) => {
        $(
            impl Diff for $ty {
                type Memo = Box<str>;

                fn into_memo(self) -> Box<str> {
                    AsRef::<str>::as_ref(self).into()
                }

                fn diff(self, memo: &mut Box<str>) -> bool {
                    if AsRef::<str>::as_ref(self) != &**memo {
                        *memo = AsRef::<str>::as_ref(self).into();
                        true
                    } else {
                        false
                    }
                }
            }
        )*
    };
}

macro_rules! impl_diff {
    ($($ty:ty),*) => {
        $(
            impl Diff for $ty {
                type Memo = $ty;

                fn into_memo(self) -> $ty {
                    self
                }

                fn diff(self, memo: &mut $ty) -> bool {
                    if self != *memo {
                        *memo = self;
                        true
                    } else {
                        false
                    }
                }
            }
        )*
    };
}

impl_diff_str!(&str, &String, &Box<str>);
impl_diff!(bool, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64);

/// Smart [`View`] that only updates its content when the reference to T has changed.
/// See [`ref`](crate::keywords::ref).
#[repr(transparent)]
pub struct Ref<T: ?Sized>(T);

impl<T: ?Sized> Deref for Ref<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> AsRef<T> for Ref<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> Diff for &Ref<T> {
    type Memo = NonNull<()>;

    fn into_memo(self) -> Self::Memo {
        NonNull::from(&self.0).cast()
    }

    fn diff(self, memo: &mut Self::Memo) -> bool {
        let ptr = NonNull::from(&self.0).cast();

        if ptr != *memo {
            *memo = ptr;
            true
        } else {
            false
        }
    }
}

/// Smart [`View`] that never performs diffing and instead always triggers
/// updates.
///
/// See [`use`](crate::keywords::use)
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Eager<T>(pub(crate) T);

/// Smart [`View`] that never performs diffing and instead never triggers
/// updates.
///
/// See [`static`](crate::keywords::static)
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Static<T>(pub(crate) T);

macro_rules! impl_no_diff {
    ($name:ident, $update:expr) => {
        impl<T> Deref for $name<T> {
            type Target = T;

            fn deref(&self) -> &T {
                &self.0
            }
        }

        impl<T> View for $name<T>
        where
            T: Value<TextContent> + IntoText + Copy,
        {
            type Product = Node;

            fn build(self, p: In<Node>) -> Out<Node> {
                p.put(self.into_text())
            }

            fn update(self, node: &mut Node) {
                if $update {
                    self.0.set_prop(TextContent, node);
                }
            }
        }

        impl<T, P> Attribute<P> for $name<T>
        where
            T: Value<P>,
        {
            type Product = ();

            fn build(self) {}

            fn build_in(self, prop: P, node: &Node) {
                self.0.set_prop(prop, node);
            }

            fn update_in(self, prop: P, node: &Node, _: &mut ()) {
                if $update {
                    self.0.set_prop(prop, node);
                }
            }
        }

        impl<T> Diff for $name<T>
        where
            T: Copy,
        {
            type Memo = ();

            fn into_memo(self) {}

            fn diff(self, _: &mut ()) -> bool {
                $update
            }
        }

        impl AsRef<str> for $name<&str> {
            fn as_ref(&self) -> &str {
                self.0
            }
        }

        impl View for $name<String> {
            type Product = Node;

            fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
                p.put(self.into_text())
            }

            fn update(self, p: &mut Self::Product) {
                if $update {
                    self.0.set_prop(TextContent, p);
                }
            }
        }
    };
}

impl_no_diff!(Eager, true);
impl_no_diff!(Static, false);
