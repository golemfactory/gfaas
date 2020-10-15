//! This crate allows you to distributed heavy-workload functions to the Golem Network (or any
//! other compatible backend when support for it is added in the future).
//!
//! ## Quick start
//!
//! The usage is pretty straightforward. In your `Cargo.toml`, put `gfaas` as your dependency
//!
//! ```toml
//! # Cargo.toml
//! [dependencies]
//! gfaas = "0.1"
//! ```
//!
//! You can now annotate some heavy-workload function to be distributed on the Golem Network
//! like so
//!
//! ```rust,ignore
//! use gfaas::remote_fn;
//!
//! #[remote_fn]
//! fn hello(input: String) -> String {
//!     // let's make the input all-caps
//!     input.to_uppercase().to_string()
//! }
//!
//! #[actix_rt::main]
//! async fn main() {
//!     let input = "hey there gfaas";
//!     let output = hello("hey there gfaas".to_string()).await.unwrap();
//!     assert_eq!(input.to_uppercase(), output)
//! }
//! ```
//!
//! In order to compile your code you'll need to use our custom wrapper on top of `cargo` called
//! `gfaas`. You can install the tool with `cargo-install` like so
//!
//! ```sh
//! cargo install gfaas-cli
//! ```
//!
//! Then, you can use `gfaas` like you would use `cargo`. So, to build and run on Golem Network,
//! you'd execute
//!
//! ```sh
//! gfaas run
//! ```
//!
//! ## Notes about `gfaas::remote_fn`
//!
//! When you annotate a function with `gfaas::remote_fn` attribute, it gets expanded into a
//! full-fledged async function which is fallible. So for instance, the following function
//!
//! ```rust,ignore
//! use gfaas::remote_fn;
//!
//! #[remote_fn]
//! fn hello(input: String) -> String;
//! ```
//!
//! expands into
//!
//! ```rust,ignore
//! async fn hello(input: String) -> Result<String, gfaas::Error>;
//! ```
//!
//! Therefore, it is important to remember that you need to run the function in an async block
//! and in order to get the result of your function back, you need to unpack it from the outer
//! `Result` type.
//!
//! The asyncness and the `Result` are there due to the nature of any distributed computation
//! run on top of some network of nodes: it may fail due to reasons not related to your app
//! such as network downtime, etc.
//!
//! Furthermore, the input and output arguments of your function have to be serializable, and
//! so they are expected to derive `serde::Serialize` and `serde::Deserialize` traits.
//!
//! ## Notes about `gfaas` build tool and adding dependecies for your functions
//!
//! The reason that a custom wrapper around `cargo` is needed, is because the function
//! annotated with `gfaas::remote_fn`, under-the-hood is actually automatically cross-compiled
//! into a WASI binary.
//!
//! In addition, since the functions are cross-compiled to WASI, you need to install
//! `wasm32-wasi` target in your used Rust toolchain. Furthermore, for that same reason, not
//! all crates are compatible with WASI yet, but you can manually specify which crates you
//! want your functions to depend on by adding a `[gfaas_dependencies]` section to your `Cargo.toml`
//!
//! ```toml
//! # Cargo.toml
//! [package]
//! author = "Jakub Konka"
//!
//! [dependecies]
//! actix = "1"
//!
//! [gfaas_dependencies]
//! log = "0.4"
//! ```
//!
//! ## Notes on running your app locally (for testing)
//!
//! It is well known that prior to launching our app on some distributed network of nodes, it
//! is convenient to first test the app locally in search of bugs and errors. This is also
//! possible with `gfaas`. In order to force your app to run locally, simply pass in
//! `GFAAS_RUN=local` env variable. For example, to run locally using the `gfaas` build tool
//! you would
//!
//! ```sh
//! GFAAS_RUN=local gfaas run
//! ```
//!
//! This will spawn all of your annotated functions in separate threads on your machine locally,
//! so you can verify that everything works as expected prior to launching the tasks on the
//! Golem Network.
//!
//! ## Examples
//!
//! A couple illustrative examples of how to use this crate can be found in the `examples/`
//! directory. All examples require `gfaas` build tool to be built.

pub mod __private {
    //! This is a private module. The stability of this API is not guaranteed and may change
    //! without notice in the future.
    pub use anyhow;
    pub use futures;
    pub use serde_json;
    pub use tempfile;
    pub use tokio;
    pub use ya_agreement_utils;
    pub use ya_runtime_wasi;
    pub use yarapi;

    #[allow(unused)]
    pub mod package {
        //! This private module describes the structures concerning Yagna packages.
        use anyhow::Result;
        use std::{
            fs,
            io::{Cursor, Write},
            path::Path,
        };
        use zip::{write::FileOptions, CompressionMethod, ZipWriter};

        /// Represents Yagna package which internally is represented as a zip archive.
        pub struct Package {
            zip_writer: ZipWriter<Cursor<Vec<u8>>>,
            options: FileOptions,
            module_name: Option<String>,
        }

        impl Package {
            /// Creates new empty Yagna package.
            pub fn new() -> Self {
                let options = FileOptions::default().compression_method(CompressionMethod::Stored);
                let zip_writer = ZipWriter::new(Cursor::new(Vec::new()));

                Self {
                    zip_writer,
                    options,
                    module_name: None,
                }
            }

            /// Adds a Wasm modules from path.
            pub fn add_module_from_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
                let module_name = path
                    .as_ref()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let contents = fs::read(path.as_ref())?;
                self.zip_writer
                    .start_file(&module_name, self.options.clone())?;
                self.zip_writer.write(&contents)?;
                self.module_name = Some(module_name);

                Ok(())
            }

            /// Write the package to file at the given path.
            pub fn write<P: AsRef<Path>>(mut self, path: P) -> Result<()> {
                // create manifest
                let comps: Vec<_> = self.module_name.as_ref().unwrap().split('.').collect();
                let manifest = serde_json::json!({
                    "id": "custom",
                    "name": "custom",
                    "entry-points": [{
                        "id": comps[0],
                        "wasm-path": self.module_name.unwrap(),
                    }],
                    "mount-points": [{
                        "rw": "workdir",
                    }]
                });
                self.zip_writer
                    .start_file("manifest.json", self.options.clone())?;
                self.zip_writer.write(&serde_json::to_vec(&manifest)?)?;

                let finalized = self.zip_writer.finish()?.into_inner();
                fs::write(path.as_ref(), finalized)?;

                Ok(())
            }
        }
    }
}

/// The bread and butter of this crate.
///
/// ## Specifying custom Golem datadir and budget
///
/// It is possible, via the attribute, to specify a custom Golem datadir location and budget
///
/// ```rust,ignore
/// use gfaas::remote_fn;
///
/// #[remote_fn(
///     datadir = "/Users/kubkon/golem/datadir",
///     budget = 100,
/// )]
/// fn hello(input: String) -> String;
/// ```
pub use gfaas_macro::remote_fn;

/// Re-export of `anyhow::Error` which is the default type returned by the expanded
/// `gfaas::remote_fn`-annotated function.
pub use anyhow::Error;
