// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use web_sys::Event;

use crate::{event::EventCast, internal, Mountable, View};

struct RuntimeData<P, F, T> {
    product: P,
    update: F,
    trigger: T,
}

trait Runtime {
    fn update(&mut self, ctx: Option<&mut Context>);
}

impl<P, F, T> Runtime for RuntimeData<P, F, T>
where
    F: Fn(NonNull<P>),
    T: Fn(NonNull<P>, &mut Context) -> Option<Then>,
{
    fn update(&mut self, ctx: Option<&mut Context>) {
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

pub struct Context {
    eid: EventId,
    event: Event,
    state: Option<NonNull<StateFrame>>,
}

struct StateFrame {
    sid: StateId,
    ptr: *mut (),
    next: Option<NonNull<Self>>,
}

impl Context {
    const fn new(eid: EventId, event: Event) -> Self {
        Context {
            eid,
            event,
            state: None,
        }
    }

    pub(crate) fn eid(&self) -> EventId {
        self.eid
    }

    pub(crate) fn event<E>(&self) -> &E
    where
        E: EventCast,
    {
        unsafe { &*(&self.event as *const _ as *const E) }
    }

    pub(crate) fn with_state<F, S>(&mut self, sid: StateId, ptr: *mut S, then: F) -> Option<Then>
    where
        F: FnOnce(&mut Self) -> Option<Then>,
    {
        // We assign current state stack frame to the next pointer
        let state = StateFrame {
            sid,
            ptr: ptr as *mut _,
            next: self.state,
        };

        // Then we replace it with a temporary reference to the new frame
        self.state = Some(NonNull::from(&state));

        let ret = then(self);

        // Finally we restore the old frame, discarding the temporary new one
        self.state = state.next;

        ret
    }

    pub(crate) fn get_state_ptr(&mut self, sid: StateId) -> Option<*mut ()> {
        let mut probe = self.state;

        while let Some(state) = probe {
            let state = unsafe { state.as_ref() };

            if state.sid == sid {
                return Some(state.ptr);
            }

            probe = state.next;
        }

        None
    }
}

pub trait Trigger {
    fn trigger(&self, _: &mut Context) -> Option<Then> {
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
        update: move |mut p: NonNull<_>| unsafe { render().update(p.as_mut()) },
        trigger: move |mut p: NonNull<_>, ctx: &mut Context| {
            let p: &mut V::Product = unsafe { p.as_mut() };

            p.trigger(ctx)
        },
    });

    internal::append_body(runtime.product.js());

    let runtime = NonNull::from(Box::leak(runtime));

    RUNTIME.set(Some(runtime));
}

pub(crate) fn trigger(event: Event, eid: EventId) {
    if let Some(runtime) = RUNTIME.get() {
        let mut ctx = Context::new(eid, event);

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
