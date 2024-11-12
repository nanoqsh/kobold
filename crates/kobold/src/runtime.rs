// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use crate::{internal, Mountable, View};

struct RuntimeData<P, F> {
    product: P,
    update: F,
}

trait Runtime {
    fn update(&mut self);
}

impl<P, F> Runtime for RuntimeData<P, F>
where
    F: Fn(NonNull<P>),
{
    fn update(&mut self) {
        (self.update)(NonNull::from(&mut self.product))
    }
}

/// Describes whether or not a component should be rendered after state changes.
/// For uses see:
///
/// * [`Hook::bind`](crate::stateful::Hook::bind)
/// * [`IntoState::update`](crate::stateful::IntoState::update)
pub trait ShouldRender: 'static {
    fn should_render(self) -> bool;
}

/// Closures without return type always update their view.
impl ShouldRender for () {
    fn should_render(self) -> bool {
        true
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
}

#[derive(Clone, Copy)]
pub struct EventId(u32);

impl EventId {
    pub(crate) fn next() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};

        static ID: AtomicU32 = AtomicU32::new(0);

        EventId(ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Event {
    eid: EventId,
}

pub trait Trigger {
    fn trigger(&self, _: &Event) -> Option<Then> {
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
    });

    internal::append_body(runtime.product.js());

    let runtime = NonNull::from(Box::leak(runtime));

    RUNTIME.set(Some(runtime));
}

pub(crate) fn lock_update<F, R>(f: F)
where
    F: FnOnce() -> R,
    R: ShouldRender,
{
    debug_assert!(RUNTIME.get().is_none(), "Cyclical update detected");

    if let Some(runtime) = RUNTIME.take() {
        if f().should_render() {
            unsafe {
                (*runtime.as_ptr()).update();
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
