// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use std::ptr::NonNull;

use crate::state::ShouldRender;
use crate::{init, internal, In, Mountable, Out, View};

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

    let runtime = In::boxed(move |p: In<RuntimeData<_, _>>| {
        p.in_place(move |p| unsafe {
            init!(p.product @ render().build(p));
            init!(p.update = move |mut p: NonNull<_>| render().update(p.as_mut()));

            Out::from_raw(p)
        })
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
    debug_assert!(RUNTIME.get().is_some(), "Cyclical update detected");

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
