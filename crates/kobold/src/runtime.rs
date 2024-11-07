// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::UnsafeCell;

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
    F: Fn(*mut P),
{
    fn update(&mut self) {
        (self.update)(&mut self.product)
    }
}

thread_local! {
    static RUNTIME: UnsafeCell<Option<*mut dyn Runtime>> = const { UnsafeCell::new(None) };
}

/// Start the Kobold app by mounting given [`View`] in the document `body`.
pub fn start<F, V>(render: F)
where
    F: Fn() -> V + 'static,
    V: View,
{
    if let Ok(true) = RUNTIME.try_with(|rt| unsafe { (*rt.get()).is_none() }) {
        init_panic_hook();

        let runtime = In::boxed(move |p: In<RuntimeData<_, _>>| {
            p.in_place(move |p| unsafe {
                init!(p.product @ render().build(p));
                init!(p.update = move |p: *mut _| render().update(&mut *p));

                Out::from_raw(p)
            })
        });

        internal::append_body(runtime.product.js());

        RUNTIME.with(move |rt| unsafe { *rt.get() = Some(Box::into_raw(runtime)) });
    }
}

pub(crate) fn update() {
    RUNTIME.with(|rt| {
        let rt = unsafe { &mut *rt.get() };
        if let Some(runtime) = rt.take() {
            unsafe { (*runtime).update() };

            *rt = Some(runtime);
        }
    });
}

fn init_panic_hook() {
    // Only enable console hook on debug builds
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
