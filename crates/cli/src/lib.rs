use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn main() {
    match rmain() {
        Ok(()) => {}
        Err(err) => {
            println!("{}", err);
            std::process::exit(1);
        }
    }
}

fn rmain() -> Result<(), String> {
    let mut args = env::args_os().skip(2);
    let subcommand = args.next().and_then(|s| s.into_string().ok());
    match subcommand.as_ref().map(|s| s.as_str()) {
        Some("build") => {
            // Set output dir
            let out_dir = Path::new("target/debug");
            // Run cargo build
            let mut cmd = Command::new("cargo");
            cmd.arg("build")
                .env("OUT_DIR", &out_dir)
                .envs(std::env::vars())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let _cmd_out = cmd.output().unwrap();
            // Next, run cargo rustc --target=wasm32-unknown-emscripten on marked
            // files.
            let mut cmd = Command::new("rustc");
            cmd.arg("+1.38.0")
                .arg("--target=wasm32-unknown-emscripten")
                .arg("wasm.rs")
                .envs(std::env::vars())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .current_dir(out_dir);
            let _cmd_out = cmd.output().unwrap();
        }
        Some("run") => {}
        _ => print_help(),
    };
    Ok(())
}

fn print_help() -> ! {
    println!(
        "\
cargo-gfaas
Compile and run a Rust crate for the wasm32-unknown-emscripten target
for use in gWasm platform on Golem Network.

USAGE:
    cargo gfaas build [OPTIONS]
    cargo gfaas run [OPTIONS]

All options accepted are the same as that of the corresponding `cargo`
subcommands. You can run `cargo gfaas build -h` for more information to learn
about flags that can be passed to `cargo gfaas build`, which mirrors the
`cargo build` command.
"
    );
    std::process::exit(0);
}
