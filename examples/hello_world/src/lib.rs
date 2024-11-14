use kobold::prelude::*;

// #[component]
// fn hello(name: &str) -> impl View + '_ {
//     view! {
//         // No need to close tags at the end of the macro
//         <h1>"Hello "{ name }"!"
//     }
// }

// kobold::start!(|| {
//     view! {
//         <!hello name="Kobold">
//     }
// });
#[component]
fn hello(name: &str) -> impl View + '_ {
    use ::kobold::dom::Mountable as _;
    use ::kobold::event::IntoListener as _;
    use ::kobold::event::ListenerHandle as _;
    use ::kobold::reexport::wasm_bindgen;
    #[wasm_bindgen::prelude::wasm_bindgen(
        inline_js = "export function __e0_22790d91e19a0c42(a) {\nlet e0=document.createElement(\"h1\");\ne0.append(\"Hello \",a,\"!\");\nreturn e0;\n}\n"
    )]
    extern "C" {
        fn __e0_22790d91e19a0c42(a: &wasm_bindgen::JsValue) -> ::kobold::reexport::web_sys::Node;
    }
    struct TransientProduct<A> {
        a: A,
        e0: ::kobold::reexport::web_sys::Node,
    }
    impl<A> ::kobold::dom::Anchor for TransientProduct<A>
    where
        Self: 'static,
    {
        type Js = ::kobold::reexport::web_sys::HtmlElement;
        type Target = ::kobold::reexport::web_sys::Node;
        fn anchor(&self) -> &Self::Target {
            &self.e0
        }
    }
    impl<A> ::kobold::runtime::Trigger for TransientProduct<A> {}
    struct Transient<A>
    where
        A: ::kobold::View,
    {
        a: A,
    }
    impl<A> ::kobold::View for Transient<A>
    where
        A: ::kobold::View,
    {
        type Product = TransientProduct<A::Product>;
        fn build(self) -> Self::Product {
            let a = self.a.build();
            TransientProduct {
                e0: ::kobold::reexport::web_sys::Node::from(__e0_22790d91e19a0c42(a.js())),
                a,
            }
        }
        fn update(self, p: &mut Self::Product) {
            self.a.update(&mut p.a);
        }
    }

    Transient { a: name }
}

kobold::start!(|| hello("Kobold"));
