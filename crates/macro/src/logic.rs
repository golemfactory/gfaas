use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Paren;
use syn::{
    parenthesized, Block, ExprLit, FnArg, Ident, Lit, Pat, ReturnType, Token, Type, Visibility,
};

#[derive(Debug)]
pub struct GwasmFn {
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
pub struct GwasmAttr {
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
pub struct GwasmAttrs(Punctuated<GwasmAttr, Token![,]>);

impl Parse for GwasmAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(GwasmAttrs(input.parse_terminated(GwasmAttr::parse)?))
    }
}

#[derive(Debug, Default)]
struct GwasmParams {
    datadir: Option<String>,
    budget: Option<u64>, // TODO should this be a bigdecimal?
}

// TODO parse optional datadir, host ip, port and net from attributes
pub(super) fn remote_fn_impl(attrs: GwasmAttrs, f: GwasmFn, preserved: TokenStream) -> TokenStream {
    // Parse attributes
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
            "budget" => {
                let lit = attr.value.lit;
                match lit {
                    Lit::Str(s) => params
                        .budget
                        .replace(s.value().parse().expect("correct value")),
                    Lit::Int(i) => params
                        .budget
                        .replace(i.base10_parse().expect("correct value")),
                    x => panic!("invalid attribute value '{:#?}'", x),
                };
            }
            x => panic!("unexpected attribute '{}'", x),
        }
    }

    // Validate and extract arguments
    let args = validate_extract_args(f.args.iter().map(|x| x.clone()));
    // Expand into gWasm connector code
    // TODO this could potentially be unsafe (passing strings like this).
    // Perhaps this could be weeded out with a custom cargo-gaas tool.
    let fn_vis = f.vis;
    let fn_ident = f.ident;
    let fn_args = f.args;
    let fn_ret = f.ret;

    let mut subtasks = vec![];
    let args_pats: Vec<_> = args.iter().map(|(pat, _)| pat.clone()).collect();
    for pat in &args_pats {
        let ts = quote!(.push_subtask_data(Vec::from(#pat)));
        subtasks.push(ts);
    }
    let datadir = params.datadir.unwrap_or_else(|| {
        appdirs::user_data_dir(Some("golem"), Some("golem"), false)
            .expect("existing project app datadirs")
            .join("default")
            .to_str()
            .expect("valid Unicode path")
            .to_owned()
    });
    let budget = params.budget.unwrap_or(5);
    // Compute out dir
    let out_dir = env::var("GFAAS_OUT_DIR").expect("GFAAS_OUT_DIR should be defined");
    let local_testing = env::var("GFAAS_LOCAL");
    let input_data = args_pats[0].clone();
    let output = if let Ok(_) = local_testing {
        quote! {
            #fn_vis async fn #fn_ident(#fn_args) #fn_ret {
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::wasi_rt;
                use gfaas::__private::package::Package;
                use std::{fs, path::Path};


                let data = Vec::from(#input_data);

                task::spawn_blocking(move || {
                    // 0. Create temp workspace
                    let workspace = tempdir().unwrap();
                    println!("{}", workspace.path().display());

                    // 1. Prepare zip archive
                    let package_path = workspace.path().join("pkg.zip");
                    let module_name = format!("{}", stringify!(#fn_ident));
                    let wasm = Path::new(#out_dir).join("bin").join(format!("{}.wasm", module_name));
                    let mut package = Package::new();
                    package.add_module_from_path(wasm).unwrap();
                    package.write(&package_path).unwrap();

                    // 2. Deploy
                    wasi_rt::deploy(workspace.path(), &package_path).unwrap();
                    wasi_rt::start(workspace.path()).unwrap();

                    let deployment = wasi_rt::DeployFile::load(workspace.path()).unwrap();
                    let vol = deployment
                        .vols
                        .iter()
                        .find(|vol| vol.path.starts_with("/workdir"))
                        .map(|vol| workspace.path().join(&vol.name))
                        .unwrap();

                    let input_file_name = "in".to_owned();
                    let output_file_name = "out".to_owned();
                    let input_path = vol.join(&input_file_name);
                    let output_path = vol.join(&output_file_name);
                    fs::write(&input_path, data).unwrap();

                    // 3. Run
                    wasi_rt::run(
                        workspace.path(),
                        &module_name,
                        vec![
                            ["/workdir/", &input_file_name].join(""),
                            ["/workdir/", &output_file_name].join(""),
                        ],
                    ).unwrap();

                    // 4. Collect the results
                    fs::read(output_path).unwrap()
                }).await.unwrap()
            }
        }
    } else {
        quote! {
            #fn_vis async fn #fn_ident(#fn_args) #fn_ret {
                use gfaas::__private::dotenv;
                use gfaas::__private::futures::{future::FutureExt, pin_mut, select};
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::package::Package;
                use gfaas::__private::ya_requestor_sdk::{self, commands, CommandList, Image::WebAssembly, Requestor};
                use gfaas::__private::ya_agreement_utils::{constraints, ConstraintKey, Constraints};
                use std::{fs, path::Path, collections::HashMap};

                // 0. Load env vars
                dotenv::from_path(Path::new(#datadir).join(".env")).unwrap();

                // 1. Create temp workspace
                let workspace = tempdir().unwrap();

                // 2. Prepare package
                let package_path = workspace.path().join("pkg.zip");
                let module_name = format!("{}", stringify!(#fn_ident));
                let wasm = Path::new(#out_dir).join("bin").join(format!("{}.wasm", module_name));
                let mut package = Package::new();
                package.add_module_from_path(wasm).unwrap();
                package.write(&package_path).unwrap();

                // 3. Prepare workspace
                let input_path = workspace.path().join("in");
                let output_path = workspace.path().join("out");
                fs::write(&input_path, Vec::from(#input_data)).unwrap();

                // 4. Run
                let requestor = Requestor::new(
                    "custom",
                    WebAssembly((1, 0, 0).into()),
                    ya_requestor_sdk::Package::Archive(package_path)
                )
                .with_max_budget_gnt(#budget)
                .with_constraints(constraints![
                    "golem.inf.mem.gib" > 0.5,
                    "golem.inf.storage.gib" > 1.0,
                    "golem.com.pricing.model" == "linear",
                ])
                .with_tasks(vec![commands! {
                    upload(&input_path, "/workdir/in");
                    run(module_name, "/workdir/in", "/workdir/out");
                    download("/workdir/out", &output_path);
                }].into_iter())
                .on_completed(|outputs: HashMap<String, String>| {
                    println!("{:#?}", outputs);
                })
                .run()
                .fuse();

                let ctrl_c = actix_rt::signal::ctrl_c().fuse();

                pin_mut!(requestor, ctrl_c);

                select! {
                    comp_res = requestor => {
                        let _ = comp_res.unwrap();
                        fs::read(&output_path).unwrap()
                    }
                    _ = ctrl_c => panic!("interrupted: ctrl-c detected!"),
                }
            }
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
        #preserved

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
    let out_path = Path::new(&out_dir)
        .join("gfaas_modules")
        .join("src")
        .join("bin")
        .join(format!("{}.rs", fn_ident.to_string()));
    let mut out = File::create(out_path).unwrap_or_else(|_| {
        panic!(
            "generating Wasm src file {}",
            [&out_dir, "gfaas.rs"].join("/")
        )
    });
    writeln!(out, "{}", contents).unwrap();

    output
}
