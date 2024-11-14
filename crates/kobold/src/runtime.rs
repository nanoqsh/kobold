// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use web_sys::Event;

use crate::{internal, Mountable, View};

mod ctx;

use ctx::EventCtx;

pub use ctx::EventContext;

struct RuntimeData<P, T, U> {
    product: P,
    trigger: T,
    update: U,
}

trait Runtime {
    fn update(&mut self, ctx: Option<&mut EventCtx>);
}

impl<P, T, U> Runtime for RuntimeData<P, T, U>
where
    T: Fn(NonNull<P>, &mut EventCtx) -> Option<Then>,
    U: Fn(NonNull<P>),
{
    fn update(&mut self, ctx: Option<&mut EventCtx>) {
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
pub enum Then {
    /// This is a silent update
    Stop,
    /// Render the view after this update
    Render,
}

impl From<()> for Then {
    fn from(_: ()) -> Self {
        Then::Render
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

pub trait Trigger {
    fn trigger<C: EventContext>(&mut self, _: &mut C) -> Option<Then> {
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
        trigger: move |mut p: NonNull<_>, ctx: &mut EventCtx| {
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
        let mut ctx = EventCtx::new(eid, &event);

        unsafe { (*runtime.as_ptr()).update(Some(&mut ctx)) }
    }
}

pub(crate) fn lock_update<F, R>(f: F)
where
    F: FnOnce() -> R,
    R: Into<Then>,
{
    debug_assert!(RUNTIME.get().is_some(), "Cyclical update detected");

    if let Some(runtime) = RUNTIME.take() {
        if let Then::Render = f().into() {
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
