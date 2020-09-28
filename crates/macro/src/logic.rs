use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{collections::VecDeque, env, fs::File, io::Write, path::Path};
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

fn validate_extract_return_type(output: &ReturnType) -> Box<Type> {
    match output {
        ReturnType::Default => panic!("functions returning unit type () are unsupported"),
        ReturnType::Type(_, tt) => tt.clone(),
    }
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
    let return_type = validate_extract_return_type(&f.ret);

    let datadir = params.datadir.unwrap_or_else(|| {
        appdirs::user_data_dir(Some("golem"), Some("golem"), false)
            .expect("existing project app datadirs")
            .join("default")
            .to_str()
            .expect("valid Unicode path")
            .to_owned()
    });
    let budget = params.budget.unwrap_or(5);

    let mut local_input_args = vec![];
    let mut remote_input_args = vec![];
    let input_file_names: Vec<_> = (0..args.len()).map(|i| format!("in{}", i)).collect();
    for (name, (arg, _)) in input_file_names.iter().zip(args.iter()) {
        let ts = quote! {
            let input_path = vol.join(#name);
            let serialized = serde_json::to_vec(&#arg).context("serializing input data")?;
            fs::write(&input_path, serialized).context("writing serialized data to file")?;
        };
        local_input_args.push(ts);

        let ts = quote! {
            let input_path = workspace.path().join(#name);
            let serialized = serde_json::to_vec(&#arg).context("serializing input data")?;
            fs::write(&input_path, serialized).context("writing serialized data to file")?;
        };
        remote_input_args.push(ts);
    }

    let mut local_input_paths = vec![];
    let mut remote_input_paths = vec![];
    let mut remote_input_names = vec![];
    for name in input_file_names {
        let ts = quote! {
            ["/workdir/", #name].join(""),
        };
        local_input_paths.push(ts);

        let ts = quote! {
            format!("/workdir/{}", #name)
        };
        remote_input_names.push(ts);

        let ts = quote! {
            upload(workspace.path().join(#name), format!("/workdir/{}", #name));
        };
        remote_input_paths.push(ts);
    }

    let output = quote! {
        #fn_vis async fn #fn_ident(#fn_args) -> std::result::Result<#return_type, gfaas::Error> {
            enum RunType {
                Local,
                Golem,
            }

            let run_type = match std::env::var("GFAAS_RUN") {
                Ok(var) => if var == "local" { RunType::Local } else { RunType::Golem },
                Err(_) => RunType::Golem,
            };

            if let RunType::Local = run_type {
                use gfaas::__private::anyhow::{anyhow, Context};
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::ya_runtime_wasi;
                use gfaas::__private::package::Package;
                use gfaas::__private::serde_json;
                use std::{fs, env, path::PathBuf};

                task::spawn_blocking(move || {
                    // 0. Create temp workspace
                    let workspace = tempdir().context("creating temp dir")?;

                    // 1. Prepare zip archive
                    let exe_path = env::current_exe().context("extracting path to the current exe")?;
                    let parent = exe_path
                        .parent()
                        .ok_or_else(|| anyhow!("path to the current exe without parent: '{}'", exe_path.display()))?;
                    let module_name = format!("{}", stringify!(#fn_ident));
                    let wasm = parent.join(format!("{}.wasm", module_name));
                    let package_path = workspace.path().join("pkg.zip");
                    let mut package = Package::new();
                    package.add_module_from_path(wasm).context("adding Wasm module from path")?;
                    package.write(&package_path).context("saving Yagna zip package to file")?;

                    // 2. Deploy
                    ya_runtime_wasi::deploy(workspace.path(), &package_path).context("deploying Yagna package")?;
                    ya_runtime_wasi::start(workspace.path()).context("executing Yagna start command")?;

                    let deployment = ya_runtime_wasi::DeployFile::load(workspace.path()).context("loading deployed Yagna package")?;
                    let vol = deployment
                        .vols()
                        .find(|vol| vol.path.starts_with("/workdir"))
                        .map(|vol| workspace.path().join(&vol.name))
                        .context("extracting workdir path from Yagna package")?;

                    let output_file_name = "out".to_owned();
                    let output_path = vol.join(&output_file_name);

                    #(#local_input_args)*

                    // 3. Run
                    ya_runtime_wasi::run(
                        workspace.path(),
                        &module_name,
                        vec![
                            #(#local_input_paths)*
                            ["/workdir/", &output_file_name].join(""),
                        ],
                    ).context("executing Yagna run command")?;

                    // 4. Collect the results
                    let output_data = fs::read(output_path).context("reading output data from file")?;
                    let res = serde_json::from_slice(&output_data).context("deserializing output data")?;
                    Ok(res)
                }).await?
            } else {
                use gfaas::__private::anyhow::{self, Context, anyhow};
                use gfaas::__private::dotenv;
                use gfaas::__private::futures::future::{select, FutureExt};
                use gfaas::__private::tokio::task;
                use gfaas::__private::tempfile::tempdir;
                use gfaas::__private::package::Package;
                use gfaas::__private::yarapi::{commands, requestor::{self, CommandList, Image::WebAssembly, Requestor}};
                use gfaas::__private::ya_agreement_utils::{constraints, ConstraintKey, Constraints};
                use gfaas::__private::serde_json;
                use std::{fs, env, path::{Path, PathBuf}, collections::HashMap};

                // 0. Load env vars
                dotenv::from_path(Path::new(#datadir).join(".env")).context("datadir not found")?;

                // 1. Create temp workspace
                let workspace = tempdir().context("creating temp dir")?;

                // 2. Prepare package
                let exe_path = env::current_exe().context("extracting path to the current exe")?;
                let parent = exe_path
                    .parent()
                    .ok_or_else(|| anyhow!("path to the current exe without parent: '{}'", exe_path.display()))?;
                let module_name = format!("{}", stringify!(#fn_ident));
                let wasm = parent.join(format!("{}.wasm", module_name));
                let package_path = workspace.path().join("pkg.zip");
                let mut package = Package::new();
                package.add_module_from_path(wasm).context("adding Wasm module from path")?;
                package.write(&package_path).context("saving Yagna zig package to file")?;

                // 3. Prepare workspace
                let output_path = workspace.path().join("out");

                #(#remote_input_args)*

                // 4. Run
                let requestor = Requestor::new(
                    "custom",
                    WebAssembly((0, 1, 0).into()),
                    requestor::Package::Archive(package_path)
                )
                .with_max_budget_gnt(#budget)
                .with_constraints(constraints![
                    "golem.inf.mem.gib" > 0.5,
                    "golem.inf.storage.gib" > 1.0,
                    "golem.com.pricing.model" == "linear",
                ])
                .with_tasks(vec![commands! {
                    #(#remote_input_paths)*
                    run(module_name, #(#remote_input_names),*, "/workdir/out");
                    download("/workdir/out", &output_path);
                }].into_iter())
                .on_completed(|outputs: HashMap<String, String>| {
                    println!("{:#?}", outputs);
                })
                .run();

                let ctrl_c = actix_rt::signal::ctrl_c().then(|r| async move {
                    match r {
                        Ok(_) => Err(anyhow!("interrupted: ctrl-c detected!")),
                        Err(e) => Err(anyhow::Error::from(e)),
                    }
                });
                select(requestor.boxed_local(), ctrl_c.boxed_local()).await.into_inner().0.context("running task on Yagna")?;
                let output_data = fs::read(&output_path).context("reading output data from file")?;
                let res = serde_json::from_slice(&output_data).context("deserializing output data")?;
                Ok(res)
            }
        }
    };

    let mut inputs = vec![];
    let mut input_args = VecDeque::with_capacity(args.len());
    for i in 0..args.len() {
        let in_ident = format_ident!("in{}", i);
        let ts = quote! {
            let next_arg = args.pop().unwrap();
            let #in_ident = fs::read(next_arg).unwrap();
            let #in_ident = serde_json::from_slice(&#in_ident).unwrap();
        };
        inputs.push(ts);
        input_args.push_front(quote!(#in_ident));
    }
    let args_in_order = input_args.as_slices().0;
    let contents = quote! {
        #preserved

        fn main() {
            use std::fs;
            use std::env;

            let mut args: Vec<_> = env::args().collect();
            let out = args.pop().unwrap();
            #(#inputs)*

            let res = #fn_ident(#(#args_in_order),*);
            let serialized = serde_json::to_vec(&res).unwrap();

            fs::write(out, &serialized).unwrap();
        }
    };

    // push body of the function into a Wasm module
    let out_dir = env::var("GFAAS_OUT_DIR")
        .expect("GFAAS_OUT_DIR should be defined. Did you build the project with gfaas tool?");
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
