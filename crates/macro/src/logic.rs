use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{env, fs::File, io::Write, path::Path};
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Paren,
    Block, ExprLit, FnArg, Ident, Lit, Pat, ReturnType, Token, Type, Visibility,
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

fn validate_extract_args(input: impl IntoIterator<Item = FnArg>) -> Vec<(Box<Pat>, Box<Type>)> {
    let mut args = vec![];
    for arg in input {
        let (pat, ty) = match arg {
            FnArg::Typed(arg) => {
                if arg.attrs.len() > 0 {
                    panic!("attributes around function arguments are unsupported");
                }
                let pat = arg.pat;
                let ty = arg.ty;
                (pat, ty)
            }
            _ => panic!("functions taking 'self' are unsupported"),
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
    budget: Option<u64>,
}

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
                    x => panic!("invalid attribute value '{:#?}': expected string", x),
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
                    x => panic!("invalid attribute value '{:#?}': expected string or int", x),
                };
            }
            x => panic!(
                "unexpected attribute '{}': expected 'datadir' or 'budget'",
                x
            ),
        }
    }

    // Validate and extract arguments
    let args = validate_extract_args(f.args.iter().map(|x| x.clone()));
    // Expand into gWasm connector code
    let fn_vis = f.vis;
    let fn_ident = f.ident;
    let fn_args = f.args;

    let fn_ret = match f.ret {
        ReturnType::Default => panic!("unit return type () is unsupported"),
        ReturnType::Type(_, tt) => quote!(gfaas::__private::anyhow::Result<#tt>),
    };

    let args_pats: Vec<_> = args.iter().map(|(pat, _)| pat.clone()).collect();
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
    let input_data = args_pats[0].clone();
    let output = quote! {
        #fn_vis async fn #fn_ident(#fn_args) -> #fn_ret {
            enum RunType {
                Local,
                Golem,
            }

            let run_type = match std::env::var("GFAAS_RUN") {
                Ok(var) => if var == "local" { RunType::Local } else { RunType::Golem },
                Err(_) => RunType::Golem,
            };

            if let RunType::Local = run_type {
                use gfaas::__private::anyhow::Context;
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::wasi_rt;
                use gfaas::__private::package::Package;
                use gfaas::__private::serde_json;
                use std::{fs, path::Path};

                task::spawn_blocking(move || {
                    // 0. Create temp workspace
                    let workspace = tempdir().context("creating temp dir")?;
                    println!("{}", workspace.path().display());

                    // 1. Prepare zip archive
                    let package_path = workspace.path().join("pkg.zip");
                    let module_name = format!("{}", stringify!(#fn_ident));
                    let wasm = Path::new(#out_dir).join("bin").join(format!("{}.wasm", module_name));
                    let mut package = Package::new();
                    package.add_module_from_path(wasm).context("adding Wasm module from path")?;
                    package.write(&package_path).context("saving Yagna zip package to file")?;

                    // 2. Deploy
                    wasi_rt::deploy(workspace.path(), &package_path).context("deploying Yagna package")?;
                    wasi_rt::start(workspace.path()).context("executing Yagna start command")?;

                    let deployment = wasi_rt::DeployFile::load(workspace.path()).context("loading deployed Yagna package")?;
                    let vol = deployment
                        .vols
                        .iter()
                        .find(|vol| vol.path.starts_with("/workdir"))
                        .map(|vol| workspace.path().join(&vol.name))
                        .context("extracting workdir path from Yagna package")?;

                    let input_file_name = "in".to_owned();
                    let output_file_name = "out".to_owned();
                    let input_path = vol.join(&input_file_name);
                    let output_path = vol.join(&output_file_name);
                    let serialized = serde_json::to_vec(&#input_data).context("serializing input data")?;
                    fs::write(&input_path, serialized).context("writing serialized data to file")?;

                    // 3. Run
                    wasi_rt::run(
                        workspace.path(),
                        &module_name,
                        vec![
                            ["/workdir/", &input_file_name].join(""),
                            ["/workdir/", &output_file_name].join(""),
                        ],
                    ).context("executing Yagna run command")?;

                    // 4. Collect the results
                    let output_data = fs::read(output_path).context("reading output data from file")?;
                    let res = serde_json::from_slice(&output_data).context("deserializing output data")?;

                    Ok(res)
                }).await?
            } else {
                use gfaas::__private::anyhow::{Context, anyhow};
                use gfaas::__private::dotenv;
                use gfaas::__private::futures::{future::FutureExt, pin_mut, select};
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::package::Package;
                use gfaas::__private::ya_requestor_sdk::{self, commands, CommandList, Image::WebAssembly, Requestor};
                use gfaas::__private::ya_agreement_utils::{constraints, ConstraintKey, Constraints};
                use gfaas::__private::serde_json;
                use std::{fs, path::Path, collections::HashMap};

                // 0. Load env vars
                dotenv::from_path(Path::new(#datadir).join(".env")).context("datadir not found")?;

                // 1. Create temp workspace
                let workspace = tempdir().context("creating temp dir")?;

                // 2. Prepare package
                let package_path = workspace.path().join("pkg.zip");
                let module_name = format!("{}", stringify!(#fn_ident));
                let wasm = Path::new(#out_dir).join("bin").join(format!("{}.wasm", module_name));
                let mut package = Package::new();
                package.add_module_from_path(wasm).context("adding Wasm module from path")?;
                package.write(&package_path).context("saving Yagna zig package to file")?;

                // 3. Prepare workspace
                let input_path = workspace.path().join("in");
                let output_path = workspace.path().join("out");
                let serialized = serde_json::to_vec(&#input_data).context("serializing input data")?;
                fs::write(&input_path, serialized).context("writing serialized data to file")?;

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
                        let _ = comp_res.context("running task on Yagna")?;
                        let output_data = fs::read(&output_path).context("reading output data from file")?;
                        let res = serde_json::from_slice(&output_data).context("deserializing output data")?;

                        Ok(res)
                    }
                    _ = ctrl_c => Err(anyhow!("interrupted: ctrl-c detected!")),
                }
            }
        }
    };

    let mut inputs = vec![];
    let mut input_args = vec![];
    for i in 0..args.len() {
        let in_ident = format_ident!("in{}", i);
        let ts = quote! {
            let next_arg = args.pop().unwrap();
            let mut f = File::open(next_arg).unwrap();
            let mut #in_ident = Vec::new();
            f.read_to_end(&mut #in_ident).unwrap();
            let #in_ident = serde_json::from_slice(&#in_ident).unwrap();
        };
        inputs.push(ts);
        input_args.push(quote!(#in_ident));
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
            let serialized = serde_json::to_vec(&res).unwrap();

            let mut f = File::create(out).unwrap();
            f.write_all(&serialized).unwrap();
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
