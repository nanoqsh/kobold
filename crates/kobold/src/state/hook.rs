// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use wasm_bindgen_futures::spawn_local;

use crate::event::{EventCast, Listener};
use crate::internal::{In, Out};
use crate::state::ShouldRender;
use crate::{runtime, View};

pub struct Signal<S> {
    inner: *const UnsafeCell<S>,
    drop_flag: Weak<()>,
}

impl<S> Signal<S> {
    pub(crate) fn new(hook: &Hook<S>) -> Self {
        let inner = &hook.inner;
        let rc = unsafe { &mut *hook.drop_flag.get() }.get_or_insert_with(|| Rc::new(()));
        let drop_flag = Rc::downgrade(rc);

        Signal { inner, drop_flag }
    }

    /// Update the state behind this `Signal`.
    ///
    /// ```
    /// # use kobold::prelude::*;
    /// fn example(count: Signal<i32>) {
    ///     // increment count and trigger a render
    ///     count.update(|count| *count += 1);
    ///
    ///     // increment count if less than 10, only render on change
    ///     count.update(|count| {
    ///         if *count < 10 {
    ///             *count += 1;
    ///             Then::Render
    ///         } else {
    ///             Then::Stop
    ///         }
    ///     })
    /// }
    /// ```
    pub fn update<F, O>(&self, mutator: F)
    where
        F: FnOnce(&mut S) -> O,
        O: ShouldRender,
    {
        if self.drop_flag.strong_count() == 1 {
            let state = unsafe { &mut *(*self.inner).get() };

            runtime::lock_update(move || mutator(state))
        }
    }

    /// Same as [`update`](Signal::update), but it never renders updates.
    pub fn update_silent<F>(&self, mutator: F)
    where
        F: FnOnce(&mut S),
    {
        if self.drop_flag.strong_count() == 1 {
            mutator(unsafe { &mut *(*self.inner).get() });
        }
    }

    /// Replace the entire state with a new value and trigger an update.
    pub fn set(&self, val: S) {
        self.update(move |s| *s = val);
    }
}

pub struct Hook<S> {
    inner: UnsafeCell<S>,
    drop_flag: UnsafeCell<Option<Rc<()>>>,
}

impl<S> Deref for Hook<S> {
    type Target = S;

    fn deref(&self) -> &S {
        unsafe { &*self.inner.get() }
    }
}

impl<S> Hook<S> {
    pub(crate) const fn new(inner: S) -> Self {
        Hook {
            inner: UnsafeCell::new(inner),
            drop_flag: UnsafeCell::new(None),
        }
    }

    /// Binds a closure to a mutable reference of the state. While this method is public
    /// it's recommended to use the [`bind!`](crate::bind) macro instead.
    pub fn bind<E, F, O>(&self, callback: F) -> Bound<S, F>
    where
        S: 'static,
        E: EventCast,
        F: Fn(&mut S, E) -> O + 'static,
        O: ShouldRender,
    {
        let inner = &self.inner;

        Bound { inner, callback }
    }

    pub fn bind_async<E, F, T>(&self, callback: F) -> impl Listener<E>
    where
        S: 'static,
        E: EventCast,
        F: Fn(Signal<S>, E) -> T + 'static,
        T: Future<Output = ()> + 'static,
    {
        let this = self as *const _;

        move |e| {
            // ⚠️ Safety:
            // ==========
            //
            // This is fired only as event listener from the DOM, which guarantees that
            // state is not currently borrowed, as events cannot interrupt normal
            // control flow, and `Signal`s cannot borrow state across .await points.
            let signal = Signal::new(unsafe { &*this });

            spawn_local(callback(signal, e));
        }
    }

    /// Get the value of state if state implements `Copy`. This is equivalent to writing
    /// `**hook` but conveys intent better.
    pub fn get(&self) -> S
    where
        S: Copy,
    {
        **self
    }
}

impl<'a, V> View for &'a Hook<V>
where
    &'a V: View + 'a,
{
    type Product = <&'a V as View>::Product;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        (**self).build(p)
    }

    fn update(self, p: &mut Self::Product) {
        (**self).update(p)
    }
}

pub struct Bound<'b, S, F> {
    inner: &'b UnsafeCell<S>,
    callback: F,
}

impl<S, F> Bound<'_, S, F> {
    pub fn into_listener<E, O>(self) -> impl Listener<E>
    where
        S: 'static,
        E: EventCast,
        F: Fn(&mut S, E) -> O + 'static,
        O: ShouldRender,
    {
        let Bound { inner, callback } = self;

        let inner = inner as *const UnsafeCell<S>;
        let bound = move |e| {
            // ⚠️ Safety:
            // ==========
            //
            // This is fired only as event listener from the DOM, which guarantees that
            // state is not currently borrowed, as events cannot interrupt normal
            // control flow, and `Signal`s cannot borrow state across .await points.
            runtime::lock_update(|| {
                let state = unsafe { &mut *(*inner).get() };

                callback(state, e)
            })
        };

        BoundListener {
            bound,
            _unbound: PhantomData::<F>,
        }
    }
}

impl<S, F> Clone for Bound<'_, S, F>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Bound {
            inner: self.inner,
            callback: self.callback.clone(),
        }
    }
}

impl<S, F> Copy for Bound<'_, S, F> where F: Copy {}

struct BoundListener<B, U> {
    bound: B,
    _unbound: PhantomData<U>,
}

impl<B, U, E> Listener<E> for BoundListener<B, U>
where
    B: Listener<E>,
    E: EventCast,
    Self: 'static,
{
    type Product = B::Product;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        self.bound.build(p)
    }

    fn update(self, p: &mut Self::Product) {
        // No need to update zero-sized closures.
        //
        // This is a const branch that should be optimized away.
        if size_of::<U>() != 0 {
            self.bound.update(p);
        }
    }
}
