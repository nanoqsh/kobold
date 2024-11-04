use std::cell::{Cell, UnsafeCell};
use std::rc::{Rc, Weak};

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
                let view = render();

                init!(p.product @ view.build(p));
                init!(p.update = move |p: *mut _| render().update(&mut *p));

                Out::from_raw(p)
            })
        });

        internal::append_body(runtime.product.js());

        RUNTIME.with(move |rt| unsafe { *rt.get() = Some(Box::into_raw(runtime)) });
    }
}

fn update() {
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

// --------------- NEW STATEFUL MOD? ------------

use std::marker::PhantomData;
use std::ops::Deref;

use crate::event::{EventCast, Listener};
use crate::stateful::{IntoState, ShouldRender};

use wasm_bindgen::JsValue;

pub fn stateful<'a, S, F, V>(
    state: S,
    render: F,
) -> Stateful<S, impl Fn(*const Hook<S::State>) -> V>
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
            let state = init!(p.state = Hook::new(UnsafeCell::new(self.state.init())));

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

impl<S, R> Stateful<S, R>
where
    S: IntoState,
{
    pub fn once<F, P>(self, handler: F) -> Once<S, R, F>
    where
        F: FnOnce(Signal<S::State>) -> P,
    {
        Once {
            with_state: self,
            handler,
        }
    }
}

pub struct Once<S, R, F> {
    with_state: Stateful<S, R>,
    handler: F,
}

pub struct OnceProduct<S, P, D> {
    inner: StatefulProduct<S, P>,
    _no_drop: D,
}

impl<S, P, D> Mountable for OnceProduct<S, P, D>
where
    StatefulProduct<S, P>: Mountable,
    D: 'static,
{
    type Js = <StatefulProduct<S, P> as Mountable>::Js;

    fn js(&self) -> &JsValue {
        self.inner.js()
    }

    fn unmount(&self) {
        self.inner.unmount()
    }

    fn replace_with(&self, new: &JsValue) {
        self.inner.replace_with(new);
    }
}

impl<S, R, F, V, D> View for Once<S, R, F>
where
    S: IntoState,
    R: Fn(*const Hook<S::State>) -> V,
    F: FnOnce(Signal<S::State>) -> D,
    V: View,
    D: 'static,
{
    type Product = OnceProduct<S::State, V::Product, D>;

    fn build(self, p: In<Self::Product>) -> Out<Self::Product> {
        p.in_place(|p| unsafe {
            let product = init!(p.inner @ self.with_state.build(p));

            let _no_drop = (self.handler)(Signal::new(&product.state));

            init!(p._no_drop = _no_drop);

            Out::from_raw(p)
        })
    }

    fn update(self, p: &mut Self::Product) {
        self.with_state.update(&mut p.inner)
    }
}

// --------------------- Hook stuff ----------

pub struct Signal<S> {
    inner: *const UnsafeCell<S>,
    drop_flag: Weak<()>,
}

impl<S> Signal<S> {
    fn new(hook: &Hook<S>) -> Self {
        let inner = &hook.inner;
        let rc = unsafe { &mut *hook.drop_flag.get() }.get_or_insert_with(|| Rc::new(()));
        let drop_flag = Rc::downgrade(rc);

        Signal { inner, drop_flag }
    }

    pub fn update<F, O>(&self, mutator: F)
    where
        F: FnOnce(&mut S) -> O,
        O: ShouldRender,
    {
        if self.drop_flag.strong_count() == 1 {
            // TODO: Use WithCell here!
            // if inner.state.with(mutator).should_render() {
            if mutator(unsafe { &mut *(*self.inner).get() }).should_render() {
                update();
            }
        }
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

// impl<S> Drop for Hook<S> {
//     fn drop(&mut self) {
//         i
//         if self.drop_flag.get() != !0 {
//             STATE_STACK.with(|stack| {
//                 let stack = unsafe { &mut *stack.get() };

//                 stack.pop();
//             })
//         }
//     }
// }

impl<S> Hook<S> {
    const fn new(inner: UnsafeCell<S>) -> Self {
        Hook {
            inner,
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
            let state = unsafe { &mut *(*inner).get() };

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
        if size_of::<U>() != 0 {
            self.bound.update(p);
        }
    }
}
