mod logic;

extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn remote_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    logic::remote_fn_impl(attr.into(), item.into()).into()
}
