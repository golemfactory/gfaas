use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use structopt::{clap::AppSettings, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "gfaas",
    author = "Jakub Konka <kubkon@golem.network>",
    about = "Compile and run a Rust crate for the wasm32-unknown-emscripten target for use in gWasm platform on Golem Network.",
    version = env!("CARGO_PKG_VERSION"),
    global_settings = &[
        AppSettings::VersionlessSubcommands,
        AppSettings::ColoredHelp
    ]
)]
struct Opt {
    #[structopt(subcommand)]
    cmd: Subcommand,
}

#[derive(Debug, StructOpt)]
enum Subcommand {
    /// Compile a local package and all of its dependencies
    Build {
        /// Build artifacts in release mode, with optimizations
        #[structopt(long)]
        release: bool,
        /// Pass additional arguments directly to cargo build command
        #[structopt()]
        args: Vec<String>,
    },
    Run {
        /// Run in release mode, with optimizations
        #[structopt(long)]
        release: bool,
        /// Pass additional arguments directly to cargo run command
        #[structopt()]
        args: Vec<String>,
    },
    Clean {
        /// Pass additional arguments directly to cargo run command
        #[structopt()]
        args: Vec<String>,
    },
}

fn main() {
    let opt = Opt::from_args();
    // Get cwd
    let cwd = env::current_dir().unwrap();
    match opt.cmd {
        Subcommand::Build { release, args } => build(&cwd, release, &args),
        Subcommand::Run { release, args } => run(&cwd, release, &args),
        Subcommand::Clean { args } => clean(&cwd, &args),
    }
}

fn build(cwd: &Path, release: bool, args: &Vec<String>) {
    // Specify output dir
    let out_dir = cwd.join(format!(
        "target/{}",
        if release { "release" } else { "debug" }
    ));
    // Run cargo build
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        // TODO We don't want the user to pass `--release` using aux cargo args,
        // so let's filter it out for now. In the future, we might want to
        // throw an error instead.
        .args(
            args.iter()
                .filter(|x| x.as_str() != "--release" && !x.contains("--target-dir")),
        )
        .env("CARGO_TARGET_DIR", "target")
        .env("GFAAS_OUT_DIR", &out_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if release {
        cmd.arg("--release");
    }
    let _cmd_out = cmd.output().unwrap();
    // Next, run cargo rustc --target=wasm32-unknown-emscripten on marked
    // files.
    let mut cmd = Command::new("rustc");
    cmd.arg("+1.38.0")
        .arg("--target=wasm32-unknown-emscripten")
        .arg("gfaas.rs")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(out_dir);
    if release {
        cmd.arg("-O");
    } else {
        cmd.arg("-g");
    }
    let _cmd_out = cmd.output().unwrap();
}

fn run(cwd: &Path, release: bool, args: &Vec<String>) {
    // We need to run cargo build first so that the Wasm artifacts are properly
    // generated.
    build(cwd, release, args);
    // Specify output dir
    let out_dir = cwd.join(format!(
        "target/{}",
        if release { "release" } else { "debug" }
    ));
    // Run cargo run
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        // TODO We don't want the user to pass `--release` using aux cargo args,
        // so let's filter it out for now. In the future, we might want to
        // throw an error instead.
        .args(
            args.iter()
                .filter(|x| x.as_str() != "--release" && !x.contains("--target-dir")),
        )
        .env("CARGO_TARGET_DIR", "target")
        .env("GFAAS_OUT_DIR", &out_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if release {
        cmd.arg("--release");
    }
    let _cmd_out = cmd.output().unwrap();
}

fn clean(cwd: &Path, args: &Vec<String>) {
    let mut cmd = Command::new("cargo");
    cmd.arg("clean")
        .args(args.iter().filter(|x| !x.contains("--target-dir")))
        .env("CARGO_TARGET_DIR", "target")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(cwd);
    let _cmd_out = cmd.output().unwrap();
}
