use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::io::Write;
use std::process::{Command, Stdio};
use syn::{parse_macro_input, FnArg, ItemFn};
use uuid::Uuid;

pub(super) fn remote_fn_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item2 = parse_macro_input!(item as ItemFn);
    let full = item2.clone();
    let vis = item2.vis;
    let ident = item2.sig.ident;
    let ident_local = format_ident!("__local_{}", ident);
    let inputs = item2.sig.inputs;
    let mut args = Vec::new();
    for input in &inputs {
        let arg = match input {
            FnArg::Typed(arg) => arg.pat.clone(),
            _ => panic!("Oh no!"),
        };
        args.push(arg)
    }
    let output = item2.sig.output;
    let body = item2.block;

    // let run_id = Uuid::new_v4();
    let run_id = "_wasm";
    // TODO here goes the Golem connector logic
    let hmm = format!("{}", run_id);
    let golem_res = quote! {
        #vis fn #ident(#inputs) #output {
            use gwasm_api::prelude::*;
            use std::fs;
            use std::path::Path;
            use std::io::Read;

            struct ProgressTracker;

            impl ProgressUpdate for ProgressTracker {
                fn update(&mut self, progress: f64) {
                    println!("Current progress = {}", progress)
                }
            }

            let js = fs::read(format!("target/debug/{}.js", #hmm)).unwrap();
            let wasm = fs::read(format!("target/debug/{}.wasm", #hmm)).unwrap();
            let binary = GWasmBinary {
                js: &js,
                wasm: &wasm,
            };
            let task = TaskBuilder::new("/Users/kubkon/dev/cuddly-jumpers/workspace", binary)
                .push_subtask_data(r#in.to_vec())
                .build()
                .unwrap();
            let computed_task = compute(
                Path::new("/Users/kubkon/dev/datadir0"),
                "127.0.0.1",
                61000,
                Net::TestNet,
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
    // TODO here goes the local version of the function;
    // so blatant copy-paste
    let res = quote! {
        #vis fn #ident_local(#inputs) #output
            #body
    };
    // TODO here goes the actual contents of the function
    let contents = quote! {
        #full

        fn main() {
            use std::fs::File;
            use std::io::{Read, Write};
            use std::env;

            let mut args: Vec<_> = env::args().collect();
            let out = args.pop().unwrap();
            let r#in = args.pop().unwrap();

            let mut f = File::open(r#in).unwrap();
            let mut arg = Vec::new();
            f.read_to_end(&mut arg).unwrap();
            println!("{:?}", arg);

            let res = #ident(&arg);

            let mut f = File::create(out).unwrap();
            f.write_all(&res).unwrap();
        }
    };

    // push body of the function into a Wasm module
    let out_dir = std::path::PathBuf::from("target/debug");
    let wasm_rs = std::path::PathBuf::from(format!("{}.rs", run_id));
    let mut out = std::fs::File::create(out_dir.join(&wasm_rs)).expect("generating Wasm src file");
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
    let output = cmd.output().unwrap();

    golem_res.into()
}

trait Render {
    fn render(&self) -> String;
}

impl Render for proc_macro2::Punct {
    fn render(&self) -> String {
        let mut out = self.to_string();
        if let proc_macro2::Spacing::Alone = self.spacing() {
            out += " ";
        }
        out
    }
}

impl Render for proc_macro2::Group {
    fn render(&self) -> String {
        let (delim_open, delim_close) = match self.delimiter() {
            proc_macro2::Delimiter::Brace => ("{", "}"),
            proc_macro2::Delimiter::Parenthesis => ("(", ")"),
            proc_macro2::Delimiter::Bracket => ("[", "]"),
            _ => unimplemented!("Delimiter::None"),
        };
        let inner = self.stream().render();
        format!("{}{}{}", delim_open, inner, delim_close)
    }
}

impl Render for proc_macro2::Ident {
    fn render(&self) -> String {
        format!("{} ", self)
    }
}

impl Render for proc_macro2::Literal {
    fn render(&self) -> String {
        format!("{} ", self)
    }
}

impl Render for proc_macro2::TokenTree {
    fn render(&self) -> String {
        match self {
            Self::Group(p) => p.render(),
            Self::Punct(p) => p.render(),
            Self::Ident(p) => p.render(),
            Self::Literal(p) => p.render(),
        }
    }
}

impl Render for proc_macro2::TokenStream {
    fn render(&self) -> String {
        let mut out = String::new();
        for tt in self.clone().into_iter() {
            out += &tt.render();
        }
        out
    }
}
