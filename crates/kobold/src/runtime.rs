// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use web_sys::Event;

use crate::event::EventCast;
use crate::state::Hook;
use crate::{internal, Mountable, View};

struct RuntimeData<P, T, U> {
    product: P,
    trigger: T,
    update: U,
}

trait Runtime {
    fn update(&mut self, ctx: Option<&mut ContextBase>);
}

impl<P, T, U> Runtime for RuntimeData<P, T, U>
where
    T: Fn(NonNull<P>, &mut ContextBase) -> Option<Then>,
    U: Fn(NonNull<P>),
{
    fn update(&mut self, ctx: Option<&mut ContextBase>) {
        let p = NonNull::from(&mut self.product);

        if let Some(ctx) = ctx {
            (self.trigger)(p, ctx);

            if let Some(Then::Stop) = (self.trigger)(p, ctx) {
                return;
            }
        }

        (self.update)(p);
    }
}

/// Describes whether or not a component should be rendered after state changes.
/// For uses see:
///
/// * [`Hook::bind`](crate::stateful::Hook::bind)
/// * [`IntoState::update`](crate::stateful::IntoState::update)
pub trait ShouldRender: 'static {
    fn should_render(self) -> bool;

    fn then(self) -> Then;
}

/// Closures without return type always update their view.
impl ShouldRender for () {
    fn should_render(self) -> bool {
        true
    }

    fn then(self) -> Then {
        Then::Render
    }
}

/// An enum that implements the [`ShouldRender`](ShouldRender) trait.
/// See:
///
/// * [`Hook::bind`](crate::stateful::Hook::bind)
/// * [`IntoState::update`](crate::stateful::IntoState::update)
pub enum Then {
    /// This is a silent update
    Stop,
    /// Render the view after this update
    Render,
}

impl ShouldRender for Then {
    fn should_render(self) -> bool {
        match self {
            Then::Stop => false,
            Then::Render => true,
        }
    }

    fn then(self) -> Then {
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct StateId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EventId(pub(crate) u32);

impl StateId {
    pub(crate) fn next() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};

        static ID: AtomicU32 = AtomicU32::new(0);

        StateId(ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl EventId {
    pub(crate) fn next() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};

        static ID: AtomicU32 = AtomicU32::new(0);

        EventId(ID.fetch_add(1, Ordering::Relaxed))
    }
}

struct ContextBase<'event, T = ()> {
    eid: EventId,
    event: &'event Event,
    states: T,
}

impl<'event> ContextBase<'event> {
    fn new(eid: EventId, event: &'event Event) -> Self {
        ContextBase {
            eid,
            event,
            states: (),
        }
    }
}

trait ContextHelper: Copy + 'static {
    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>;
}

impl ContextHelper for () {
    fn with_state<S, F>(&self, _: StateId, _: F) -> Option<Then>
    where
        F: Fn(&mut S) -> Option<Then>,
    {
        None
    }
}

impl<'a, T, U> ContextHelper for (NonNull<Hook<T>>, U)
where
    T: 'static,
    U: ContextHelper + 'static,
{
    fn with_state<S, F>(&self, sid: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>,
    {
        use std::any::TypeId;

        let hook = unsafe { self.0.as_ref() };

        // There might be conflicts on the hashes here, but that's okay
        // as we are going to rely on unique nature of `StateId`.
        //
        // Ideally the first condition will be evaluated at compile time
        // and this whole branch is gone if `T` isn't the same type as `S`.
        if TypeId::of::<T>() == TypeId::of::<S>() && hook.id == sid {
            let state_ptr = hook.as_ptr() as *mut S;

            return then(unsafe { &mut *state_ptr });
        }

        self.1.with_state(sid, then)
    }
}

pub trait Context {
    type Attached<'hook, S>: Context + 'hook
    where
        S: 'static,
        Self: 'hook;

    fn eid(&self) -> EventId;

    fn event<E>(&self) -> &E
    where
        E: EventCast;

    fn attach<'hook, S>(&self, hook: &'hook Hook<S>) -> Self::Attached<'hook, S>
    where
        S: 'static,
        Self: 'hook;

    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>;
}

impl<'event, T> Context for ContextBase<'event, T>
where
    T: ContextHelper,
{
    type Attached<'hook, S> = ContextBase<'hook, (NonNull<Hook<S>>, T)>
    where
        S: 'static,
        Self: 'hook;

    fn eid(&self) -> EventId {
        self.eid
    }

    fn event<E>(&self) -> &E
    where
        E: EventCast,
    {
        unsafe { &*(&self.event as *const _ as *const E) }
    }

    fn attach<'hook, S>(&self, hook: &'hook Hook<S>) -> Self::Attached<'hook, S>
    where
        S: 'static,
        Self: 'hook,
    {
        ContextBase {
            eid: self.eid,
            event: self.event,
            states: (hook.into(), self.states),
        }
    }

    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>,
    {
        self.states.with_state(id, then)
    }
}

pub trait Trigger {
    fn trigger<C: Context>(&mut self, _: &mut C) -> Option<Then> {
        None
    }
}

thread_local! {
    static INIT: Cell<bool> = const { Cell::new(false) };

    static RUNTIME: Cell<Option<NonNull<dyn Runtime>>> = const { Cell::new(None) };
}

/// Start the Kobold app by mounting given [`View`] in the document `body`.
pub fn start<F, V>(render: F)
where
    F: Fn() -> V + 'static,
    V: View,
{
    if INIT.get() {
        return;
    }
    INIT.set(true);

    init_panic_hook();

    let runtime = Box::new(RuntimeData {
        product: render().build(),
        trigger: move |mut p: NonNull<_>, ctx: &mut ContextBase| {
            let p: &mut V::Product = unsafe { p.as_mut() };

            p.trigger(ctx)
        },
        update: move |mut p: NonNull<_>| unsafe { render().update(p.as_mut()) },
    });

    internal::append_body(runtime.product.js());

    let runtime = NonNull::from(Box::leak(runtime));

    RUNTIME.set(Some(runtime));
}

pub(crate) fn trigger(eid: EventId, event: Event) {
    if let Some(runtime) = RUNTIME.get() {
        let mut ctx = ContextBase::new(eid, &event);

        unsafe { (*runtime.as_ptr()).update(Some(&mut ctx)) }
    }
}

pub(crate) fn lock_update<F, R>(f: F)
where
    F: FnOnce() -> R,
    R: ShouldRender,
{
    debug_assert!(RUNTIME.get().is_some(), "Cyclical update detected");

    if let Some(runtime) = RUNTIME.take() {
        if f().should_render() {
            unsafe {
                (*runtime.as_ptr()).update(None);
            }
        }

        RUNTIME.set(Some(runtime));
    }
}

fn init_panic_hook() {
    // Only enable console hook on debug builds
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
