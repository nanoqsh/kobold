// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use web_sys::Event;

use crate::{internal, Mountable, View};

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
    T: Fn(NonNull<P>, &mut Context),
{
    fn update(&mut self, ctx: Option<&mut Context>) {
        let p = NonNull::from(&mut self.product);

        if let Some(ctx) = ctx {
            (self.trigger)(p, ctx);

            let ContextStep::Result(Then::Render) = ctx.step else {
                return;
            };
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

#[repr(transparent)]
pub struct Step(Then);

impl Step {
    pub(crate) fn then(t: Then) -> Self {
        Step(t)
    }

    pub(crate) fn require_state() -> Self {
        Step(Then::Stop)
    }
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

    pub(crate) fn void() -> Self {
        StateId(u32::MAX)
    }
}

impl EventId {
    pub(crate) fn next() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};

        static ID: AtomicU32 = AtomicU32::new(0);

        EventId(ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Context<'a> {
    event: Option<Event>,
    step: ContextStep<'a>,
}

pub(crate) enum ContextStep<'a> {
    Init {
        eid: EventId,
    },
    StateProvision {
        sid: StateId,
        callback: &'a dyn Callback,
    },
    Result(Then),
}

impl<'a> Context<'a> {
    pub const fn new(eid: EventId, event: Event) -> Self {
        Context {
            event: Some(event),
            step: ContextStep::Init { eid },
        }
    }

    pub(crate) fn provide_state<S>(&mut self, id: StateId, state: &mut S) {
        match self.step {
            ContextStep::StateProvision { callback, sid } if sid == id => {
                let then = unsafe {
                    let event = self.event.take().unwrap_unchecked();
                    callback.handle(state as *mut _ as _, event)
                };

                self.step = ContextStep::Result(then)
            }
            _ => (),
        }
    }

    pub fn finished(&self) -> bool {
        match self.step {
            ContextStep::Result(_) => true,
            _ => false,
        }
    }
}

pub trait Trigger {
    fn trigger<'prod>(&'prod self, _: &mut Context<'prod>) {}
}

pub trait Callback {
    unsafe fn handle(&self, state: *mut (), event: Event) -> Then;
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
