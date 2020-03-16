use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Paren;
use syn::{
    parenthesized, parse_macro_input, Block, ExprLit, FnArg, Ident, Lit, Pat, ReturnType, Token,
    Type, Visibility,
};
use uuid::Uuid;

// TODO handle asyncness
#[derive(Debug)]
struct GwasmFn {
    vis: Visibility,
    fn_token: Token![fn],
    ident: Ident,
    paren_token: Paren,
    args: Punctuated<FnArg, Token![,]>,
    ret: ReturnType,
    body: Box<Block>,
}

impl Parse for GwasmFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(GwasmFn {
            vis: input.parse()?,
            fn_token: input.parse()?,
            ident: input.parse()?,
            paren_token: parenthesized!(content in input),
            args: content.parse_terminated(FnArg::parse)?,
            ret: input.parse()?,
            body: input.parse()?,
        })
    }
}

fn validate_arg_type(ty: &Type) -> bool {
    match ty {
        Type::Array(arr) => validate_arg_type(&arr.elem),
        Type::Slice(slice) => validate_arg_type(&slice.elem),
        Type::Reference(r#ref) => validate_arg_type(&r#ref.elem),
        Type::Path(path) => {
            let path = &path.path;
            if let Some(ident) = path.get_ident() {
                ident.to_string() == "u8"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn validate_extract_args(input: impl IntoIterator<Item = FnArg>) -> Vec<(Box<Pat>, Box<Type>)> {
    let mut args = vec![];
    for arg in input {
        let (pat, ty) = match arg {
            FnArg::Typed(arg) => {
                if arg.attrs.len() > 0 {
                    panic!("attributes around fn args are unsupported");
                }
                let pat = arg.pat;
                let ty = arg.ty;
                if !validate_arg_type(&ty) {
                    panic!("unsupported argument type");
                }
                (pat, ty)
            }
            _ => panic!("self params are unsupported"),
        };
        args.push((pat, ty));
    }
    args
}

#[derive(Debug)]
struct GwasmAttr {
    ident: Ident,
    eq_token: Token![=],
    value: ExprLit,
}

impl Parse for GwasmAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(GwasmAttr {
            ident: input.parse()?,
            eq_token: input.parse()?,
            value: input.parse()?,
        })
    }
}

#[derive(Debug)]
struct GwasmAttrs(Punctuated<GwasmAttr, Token![,]>);

impl Parse for GwasmAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(GwasmAttrs(input.parse_terminated(GwasmAttr::parse)?))
    }
}

#[derive(Debug, Default)]
struct GwasmParams {
    datadir: Option<String>,
    rpc_address: Option<String>,
    rpc_port: Option<u16>,
    net: Option<String>,
}

// TODO parse optional datadir, host ip, port and net from attributes
pub(super) fn remote_fn_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse attributes
    let attrs = parse_macro_input!(attr as GwasmAttrs);
    let mut params = GwasmParams::default();
    for attr in attrs.0.into_iter() {
        let attr_str = attr.ident.to_string();
        match attr_str.as_str() {
            "datadir" => {
                let lit = attr.value.lit;
                match lit {
                    Lit::Str(s) => params.datadir.replace(s.value()),
                    x => panic!("invalid attribute value '{:#?}'", x),
                };
            }
            "rpc_address" => {
                let lit = attr.value.lit;
                match lit {
                    Lit::Str(s) => params.rpc_address.replace(s.value()),
                    x => panic!("invalid attribute value '{:#?}'", x),
                };
            }
            "rpc_port" => {
                let lit = attr.value.lit;
                match lit {
                    Lit::Str(s) => params
                        .rpc_port
                        .replace(s.value().parse().expect("correct value")),
                    Lit::Int(i) => params
                        .rpc_port
                        .replace(i.base10_parse().expect("correct value")),
                    x => panic!("invalid attribute value '{:#?}'", x),
                };
            }
            "net" => {
                let lit = attr.value.lit;
                match lit {
                    Lit::Str(s) => match s.value().to_lowercase().as_str() {
                        "testnet" => params.net.replace("testnet".to_string()),
                        "mainnet" => params.net.replace("mainnet".to_string()),
                        x => panic!("invalid attribute value '{}'", x),
                    },
                    x => panic!("invalid attribute value '{:#?}'", x),
                };
            }
            x => panic!("unexpected attribute '{}'", x),
        }
    }

    // Parse body
    let preserved_item: proc_macro2::TokenStream = item.clone().into();
    let item = parse_macro_input!(item as GwasmFn);
    // Validate and extract arguments
    let args = validate_extract_args(item.args.iter().map(|x| x.clone()));
    // Expand into gWasm connector code
    let run_id = Uuid::new_v4();
    let run_id_str = format!("{}", run_id);
    let fn_vis = item.vis;
    let fn_ident = item.ident;
    let fn_args = item.args;
    let fn_ret = item.ret;
    let mut subtasks = vec![];
    for (pat, _) in &args {
        let ts = quote!(.push_subtask_data(Vec::from(#pat)));
        subtasks.push(ts);
    }
    let datadir = params
        .datadir
        .unwrap_or("/Users/kubkon/Library/Application Support/golem/default".to_string());
    let rpc_address = params.rpc_address.unwrap_or("127.0.0.1".to_string());
    let rpc_port = params.rpc_port.unwrap_or(61000);
    let net = params.net.unwrap_or("testnet".to_string());
    let output = quote! {
        #fn_vis fn #fn_ident(#fn_args) #fn_ret {
            use gwasm_api::prelude::*;
            use std::fs;
            use std::path::Path;
            use std::io::Read;

            struct ProgressTracker;

            impl ProgressUpdate for ProgressTracker {
                fn update(&mut self, _progress: f64) {}
            }

            let js = fs::read(format!("target/debug/{}.js", #run_id_str)).unwrap();
            let wasm = fs::read(format!("target/debug/{}.wasm", #run_id_str)).unwrap();
            let binary = GWasmBinary {
                js: &js,
                wasm: &wasm,
            };
            let task = TaskBuilder::new("/Users/kubkon/dev/cuddly-jumpers/workspace", binary)
                #(#subtasks)*
                .build()
                .unwrap();
            let computed_task = compute(
                Path::new(#datadir),
                #rpc_address,
                #rpc_port,
                match #net {
                    "testnet" => Net::TestNet,
                    "mainnet" => Net::MainNet,
                    _ => unreachable!(),
                },
                task,
                ProgressTracker,
            )
            .unwrap();

            let mut out = vec![];
            for subtask in computed_task.subtasks {
                for (_, mut reader) in subtask.data {
                    reader.read_to_end(&mut out).unwrap();
                }
            }
            out
        }
    };

    // TODO here goes the actual contents of the Wasm module
    let mut inputs = vec![];
    let mut input_args = vec![];
    for i in 0..args.len() {
        let in_ident = format_ident!("in{}", i);
        let ts = quote! {
            let next_arg = args.pop().unwrap();
            let mut f = File::open(next_arg).unwrap();
            let mut #in_ident = Vec::new();
            f.read_to_end(&mut #in_ident).unwrap();
        };
        inputs.push(ts);
        input_args.push(quote!(&#in_ident));
    }
    let contents = quote! {
        #preserved_item

        fn main() {
            use std::fs::File;
            use std::io::{Read, Write};
            use std::env;

            let mut args: Vec<_> = env::args().collect();
            let out = args.pop().unwrap();
            #(#inputs)*

            let res = #fn_ident(#(#input_args),*);

            let mut f = File::create(out).unwrap();
            f.write_all(&res).unwrap();
        }
    };

    // push body of the function into a Wasm module
    let out_dir = PathBuf::from("target/debug");
    let wasm_rs = PathBuf::from(format!("{}.rs", run_id));
    let mut out = File::create(out_dir.join(&wasm_rs)).expect("generating Wasm src file");
    writeln!(out, "{}", contents).unwrap();

    // compile to gWasm
    let mut cmd = Command::new("rustc");
    cmd.arg("+1.38.0")
        .arg("--target=wasm32-unknown-emscripten")
        .arg(&wasm_rs)
        .envs(std::env::vars())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir("target/debug");
    let _cmd_output = cmd.output().unwrap();

    output.into()
}
