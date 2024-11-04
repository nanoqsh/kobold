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
    static RUNTIME: UnsafeCell<Option<Box<dyn Runtime>>> = const { UnsafeCell::new(None) };
}

/// Start the Kobold app by mounting given [`View`] in the document `body`.
pub fn start<F, V>(render: F)
where
    F: Fn() -> V + 'static,
    V: View,
{
    if RUNTIME.with(|rt| unsafe { (*rt.get()).is_none() }) {
        init_panic_hook();

        let runtime = In::boxed(move |p: In<RuntimeData<_, _>>| {
            p.in_place(move |p| unsafe {
                let view = render();

                init!(p.product @ view.build(p));
                init!(p.update = move |p: *mut _| render().update(&mut *p));

                Out::from_raw(p)
            })
        });

        internal::append_body(runtime.product.js());

        RUNTIME.with(move |rt| unsafe { *rt.get() = Some(runtime) });
    }
}

pub(crate) fn update() {
    RUNTIME.with(|rt| {
        if let Some(mut runtime) = unsafe { (*rt.get()).take() } {
            runtime.update();

            unsafe { *rt.get() = Some(runtime) };
        }
    })
}

fn init_panic_hook() {
    // Only enable console hook on debug builds
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}

// --------------- NEW STATEFUL MOD? ------------

use std::ops::Deref;
use std::marker::PhantomData;

use crate::stateful::{IntoState, WithCell, ShouldRender};
use crate::event::{EventCast, Listener};

use wasm_bindgen::JsValue;

pub fn stateful<'a, S, F, V>(state: S, render: F) -> Stateful<S, impl Fn(*const Hook<S::State>) -> V>
where
    S: IntoState,
    F: Fn(&'a Hook<S::State>) -> V,
    V: View + 'a,
{
    // There is no safe way to represent a generic closure with generic return type
    // that borrows from that closure's arguments, without also slapping a lifetime.
    //
    // The `stateful` function ensures that correct lifetimes are used before we
    // erase them for the use in the `Stateful` struct.
    let render = move |hook: *const Hook<S::State>| render(unsafe { &*hook });
    Stateful { state, render }
}

pub struct Stateful<S, F> {
    state: S,
    render: F,
}

pub struct StatefulProduct<S, P> {
    state: Hook<S>,
    product: P,
}

impl<S, F, V> View for Stateful<S, F>
where
    S: IntoState,
    F: Fn(*const Hook<S::State>) -> V,
    V: View,
{
    type Product = StatefulProduct<S::State, V::Product>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        p.in_place(|p| unsafe {
            let state = init!(p.state = Hook(WithCell::new(self.state.init())));

            init!(p.product @ (self.render)(&*state).build(p));

            Out::from_raw(p)
        })
    }

    fn update(self, p: &mut Self::Product) {
        (self.render)(&p.state).update(&mut p.product)
    }
}

impl<S, P> Mountable for StatefulProduct<S, P>
where
    S: 'static,
    P: Mountable,
{
    type Js = P::Js;

    fn js(&self) -> &JsValue {
        self.product.js()
    }

    fn unmount(&self) {
        self.product.unmount()
    }

    fn replace_with(&self, new: &JsValue) {
        self.product.replace_with(new);
    }
}

#[repr(transparent)]
pub struct Hook<S>(WithCell<S>);

impl<S> Deref for Hook<S> {
    type Target = S;

    fn deref(&self) -> &S {
        unsafe { self.0.ref_unchecked() }
    }
}

impl<S> Hook<S> {
    /// Binds a closure to a mutable reference of the state. While this method is public
    /// it's recommended to use the [`bind!`](crate::bind) macro instead.
    pub fn bind<E, F, O>(&self, callback: F) -> Bound<S, F>
    where
        S: 'static,
        E: EventCast,
        F: Fn(&mut S, E) -> O + 'static,
        O: ShouldRender,
    {
        let inner = &self.0;

        Bound { inner, callback }
    }

    // pub fn bind_async<E, F, T>(&self, callback: F) -> impl Listener<E>
    // where
    //     S: 'static,
    //     E: EventCast,
    //     F: Fn(Signal<S>, E) -> T + 'static,
    //     T: Future<Output = ()> + 'static,
    // {
    //     let inner = &self.inner as *const Inner<S>;

    //     move |e| {
    //         // ⚠️ Safety:
    //         // ==========
    //         //
    //         // This is fired only as event listener from the DOM, which guarantees that
    //         // state is not currently borrowed, as events cannot interrupt normal
    //         // control flow, and `Signal`s cannot borrow state across .await points.
    //         //
    //         // This temporary `Rc` will not mess with the `strong_count` value, we only
    //         // need it to construct a `Weak` reference to `Inner`.
    //         let rc = ManuallyDrop::new(unsafe { Rc::from_raw(inner) });

    //         let signal = Signal {
    //             weak: Rc::downgrade(&*rc),
    //         };

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

pub struct Bound<'b, S, F> {
    inner: &'b WithCell<S>,
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

        let inner = inner as *const WithCell<S>;
        let bound = move |e| {
            // ⚠️ Safety:
            // ==========
            //
            // This is fired only as event listener from the DOM, which guarantees that
            // state is not currently borrowed, as events cannot interrupt normal
            // control flow, and `Signal`s cannot borrow state across .await points.
            let state = unsafe { (*inner).mut_unchecked() };

            if callback(state, e).should_render() {
                update();
            }
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
        if std::mem::size_of::<U>() != 0 {
            self.bound.update(p);
        }
    }
}
