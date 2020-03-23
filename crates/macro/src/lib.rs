extern crate proc_macro;

mod logic;

use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro_attribute]
pub fn remote_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as logic::GwasmAttrs);
    let preserved = item.clone();
    let f = parse_macro_input!(item as logic::GwasmFn);
    logic::remote_fn_impl(attrs, f, preserved.into()).into()
}
