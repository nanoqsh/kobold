// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::UnsafeCell;
// use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;

use wasm_bindgen::JsValue;
// use wasm_bindgen_futures::spawn_local;

use crate::event::{EventCast, Listener, ListenerHandle};
use crate::runtime::{self, Context, EventId, ShouldRender, StateId, Step, Then, Trigger};
use crate::{internal, View};

pub struct Signal<S> {
    inner: *const UnsafeCell<S>,
    _id: StateId,
}

impl<S> Signal<S> {
    pub(crate) fn new(hook: &Hook<S>) -> Self {
        let inner = &hook.inner;

        Signal {
            inner,
            _id: hook.id,
        }
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
        // TODO: handle StateId
        let state = unsafe { &mut *(*self.inner).get() };

        runtime::lock_update(move || mutator(state))
    }

    /// Same as [`update`](Signal::update), but it never renders updates.
    pub fn update_silent<F>(&self, mutator: F)
    where
        F: FnOnce(&mut S),
    {
        // TODO: handle StateId
        mutator(unsafe { &mut *(*self.inner).get() });
    }

    /// Replace the entire state with a new value and trigger an update.
    pub fn set(&self, val: S) {
        self.update(move |s| *s = val);
    }
}

pub struct Hook<S> {
    inner: UnsafeCell<S>,
    pub(super) id: StateId,
}

impl<S> Deref for Hook<S> {
    type Target = S;

    fn deref(&self) -> &S {
        unsafe { &*self.inner.get() }
    }
}

impl<S> Hook<S> {
    pub(crate) fn new(inner: S) -> Self {
        Hook {
            inner: UnsafeCell::new(inner),
            id: StateId::next(),
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut S {
        self.inner.get()
    }

    /// Binds a closure to a mutable reference of the state. While this method is public
    /// it's recommended to use the [`bind!`](crate::bind) macro instead.
    pub fn bind<E, F, O>(&self, callback: F) -> Bound<S, F>
    where
        S: 'static,
        E: EventCast,
        F: Fn(&mut S, &E) -> O + 'static,
        O: ShouldRender,
    {
        Bound {
            sid: self.id,
            callback,
            _marker: PhantomData,
        }
    }

    // pub fn bind_async<E, F, T>(&self, callback: F) -> impl Listener<E>
    // where
    //     S: 'static,
    //     E: EventCast,
    //     F: Fn(Signal<S>, E) -> T + 'static,
    //     T: Future<Output = ()> + 'static,
    // {
    //     let this = self as *const _;

    //     move |e| {
    //         // ⚠️ Safety:
    //         // ==========
    //         //
    //         // This is fired only as event listener from the DOM, which guarantees that
    //         // state is not currently borrowed, as events cannot interrupt normal
    //         // control flow, and `Signal`s cannot borrow state across .await points.
    //         let signal = Signal::new(unsafe { &*this });

    //         spawn_local(callback(signal, e));
    //     }
    // }

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

    fn build(self) -> Self::Product {
        (**self).build()
    }

    fn update(self, p: &mut Self::Product) {
        (**self).update(p)
    }
}

#[derive(Clone, Copy)]
pub struct Bound<S, F> {
    sid: StateId,
    callback: F,
    _marker: PhantomData<S>,
}

#[derive(Clone, Copy)]
pub struct BoundProduct<E, S, F> {
    eid: EventId,
    sid: StateId,
    callback: F,
    _marker: PhantomData<NonNull<(E, S)>>,
}

impl<E, S, F, O> Listener<E> for Bound<S, F>
where
    S: 'static,
    E: EventCast,
    F: Fn(&mut S, &E) -> O + 'static,
    O: ShouldRender,
{
    type Product = BoundProduct<E, S, F>;

    fn build(self) -> Self::Product {
        BoundProduct {
            eid: EventId::next(),
            sid: self.sid,
            callback: self.callback,
            _marker: PhantomData,
        }
    }

    fn update(self, p: &mut Self::Product) {
        p.sid = self.sid;
        p.callback = self.callback;
    }
}

impl<E, S, F, O> ListenerHandle for BoundProduct<E, S, F>
where
    S: 'static,
    E: EventCast,
    F: Fn(&mut S, &E) -> O + 'static,
    O: ShouldRender,
{
    fn js_value(&mut self) -> JsValue {
        internal::make_event_handler(self.eid.0)
    }

    unsafe fn handle(&self, state: *mut (), event: &web_sys::Event) -> Then {
        let state = &mut *(state as *mut S);
        let event = &*(event as *const _ as *const E);

        (self.callback)(state, event).then()
    }
}

impl<E, S, F, O> Trigger for BoundProduct<E, S, F>
where
    S: 'static,
    E: EventCast,
    F: Fn(&mut S, &E) -> O + 'static,
    O: ShouldRender,
{
    fn trigger<'prod>(&'prod self, ctx: &mut Context<'prod>) -> Option<Step> {
        if ctx.eid != self.eid {
            return None;
        }

        debug_assert!(ctx.sid == StateId::void());

        ctx.sid = self.sid;
        ctx.callback = Some(self);

        Some(Step::require_state())
    }
}
