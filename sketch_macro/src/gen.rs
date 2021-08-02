use std::fmt::{self, Write};
use proc_macro::{TokenStream, TokenTree, Ident, Span};
use proc_macro2::TokenStream as QuoteTokens;
use quote::quote;
use crate::dom::Node;

#[derive(Debug)]
pub enum Infallible {}

impl From<fmt::Error> for Infallible {
    fn from(err: fmt::Error) -> Infallible {
        panic!("{}", err)
    }
}

pub struct IdentFactory {
    prefix: char,
    current: usize,
}

impl IdentFactory {
    pub fn new(prefix: char) -> Self {
        Self {
            prefix,
            current: 0,
        }
    }

    pub fn next(&mut self) -> (String, QuoteTokens) {
        let string = format!("{}{}", self.prefix, self.current);
        let ident = Ident::new(&string, Span::call_site());

        self.current += 1;

        (string, TokenStream::from(TokenTree::Ident(ident)).into())
    }
}

pub struct Generator {
    var_count: usize,
    arg_factory: IdentFactory,
    pub render: String,
    pub update: String,
    args: String,
    args_tokens: Vec<QuoteTokens>,
}

impl Generator {
    pub fn new() -> Self {
        Generator {
            var_count: 0,
            arg_factory: IdentFactory::new('a'),
            render: String::new(),
            update: String::new(),
            args: String::new(),
            args_tokens: Vec::new(),
        }
    }

    pub fn generate(&mut self, dom: &Node) -> Result<String, Infallible> {
        match dom {
            Node::Text(text) => {
                let e = self.next_el();

                write!(&mut self.render, "const {}=document.createTextNode({})", e, text)?;

                Ok(e)
            }
            Node::Element(el) => {
                let e = self.next_el();

                write!(&mut self.render, "const {}=document.createElement({:?});", e, el.tag)?;

                self.append(&e, &el.children)?;

                Ok(e)
            },
            Node::Fragment(children) => {
                let e = self.next_el();

                write!(&mut self.render, "const {}=document.createDocumentFragment();", e)?;

                self.append(&e, children)?;

                Ok(e)
            }
            Node::Expression => {
                let (arg, token) = self.arg_factory.next();

                if !self.args.is_empty() {
                    self.args.push(',');
                }
                self.args.push_str(&arg);
                self.args_tokens.push(token);

                Ok(arg)
            }
        }
    }

    pub fn append(&mut self, el: &str, children: &[Node]) -> Result<(), Infallible> {
        let mut append = String::new();

        if let Some((first, rest)) = children.split_first() {
            match first {
                Node::Text(text) => append.push_str(text),
                node => append.push_str(&self.generate(node)?),
            }

            for child in rest {
                append.push(',');

                match child {
                    Node::Text(text) => append.push_str(text),
                    node => append.push_str(&self.generate(node)?),
                }
            }
        }

        write!(&mut self.render, "{}.append({});", el, append)?;

        Ok(())
    }

    pub fn render_js(&mut self, root: &str) -> (QuoteTokens, QuoteTokens) {
        let fn_name = format!("render{}", self.args.len());

        let js = format!("export function {}({}){{{}return {};}}", fn_name, self.args, self.render, root);
        let args = &self.args_tokens;

        let fn_name = Ident::new(&fn_name, Span::call_site());
        let fn_name: QuoteTokens = TokenStream::from(TokenTree::Ident(fn_name)).into();

        (fn_name.clone(), quote! {
            #[wasm_bindgen(inline_js = #js)]
            extern "C" {
                fn #fn_name(#(#args: &Node),*) -> Node;
            }
        })
    }

    fn next_el(&mut self) -> String {
        let e = format!("e{}", self.var_count);
        self.var_count += 1;
        e
    }
}
