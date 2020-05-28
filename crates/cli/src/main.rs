use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs, io};
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
        /// Local-only (for testing only)
        #[structopt(long)]
        local: bool,
        /// Build artifacts in release mode, with optimizations
        #[structopt(long)]
        release: bool,
        /// Pass additional arguments directly to cargo build command
        #[structopt()]
        args: Vec<String>,
    },
    Run {
        /// Local-only (for testing only)
        #[structopt(long)]
        local: bool,
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
        Subcommand::Build {
            local,
            release,
            args,
        } => build(&cwd, local, release, &args),
        Subcommand::Run {
            local,
            release,
            args,
        } => run(&cwd, local, release, &args),
        Subcommand::Clean { args } => clean(&cwd, &args),
    }
}

fn build(cwd: &Path, local: bool, release: bool, args: &Vec<String>) {
    // Specify output dir
    let out_dir = cwd.join(format!(
        "target/{}",
        if release { "release" } else { "debug" }
    ));
    // Fetch cargo manifest path for the root project
    let mut cmd = Command::new("cargo");
    let cmd_out = cmd.arg("metadata").output().unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&cmd_out.stdout).unwrap();
    let workspace_root = metadata["workspace_root"].as_str().unwrap();
    // Create cargo package with gfaas funcs
    let module_path = Path::new(&out_dir).join("gfaas_modules");
    let bin_path = module_path.join("src").join("bin");
    if let Err(err) = fs::create_dir_all(&bin_path) {
        match err.kind() {
            io::ErrorKind::AlreadyExists => {}
            _ => panic!("couldn't create gfaas_module dir: {}", err),
        }
    }
    // Parse manifest of the workspace and extract gfaas deps
    let manifest_path = Path::new(workspace_root).join("Cargo.toml");
    let contents = fs::read_to_string(&manifest_path).unwrap();
    let mut manifest_toml = contents.parse::<toml::Value>().unwrap();
    let manifest_toml = manifest_toml.as_table_mut().unwrap();
    let gfaas_deps = manifest_toml.remove("gfaas_dependencies").unwrap();
    let mut gfaas_toml = toml::toml! {
        [package]
        name = "gfaas_modules"
        version = "0.1.0"
    };
    gfaas_toml
        .as_table_mut()
        .unwrap()
        .insert("dependencies".to_owned(), gfaas_deps.clone().into());
    fs::write(
        module_path.join("Cargo.toml"),
        toml::to_string(&gfaas_toml).unwrap(),
    )
    .unwrap();
    // Run cargo build
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if local {
        cmd.env("GFAAS_LOCAL", "");
    }
    cmd
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
    // Next, run cargo build --target=wasm32-unknown-emscripten on gfaas_modules
    // crate.
    let mut cmd = Command::new("cargo");
    cmd.arg("+1.38.0")
        .arg("install")
        .arg("--target=wasm32-unknown-emscripten")
        .arg("--bins")
        .arg("--force")
        .arg("--root")
        .arg(out_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(module_path);
    if !release {
        cmd.arg("--debug");
    }
    let _cmd_out = cmd.output().unwrap();
}

fn run(cwd: &Path, local: bool, release: bool, args: &Vec<String>) {
    // We need to run cargo build first so that the Wasm artifacts are properly
    // generated.
    build(cwd, local, release, args);
    // Specify output dir
    let out_dir = cwd.join(format!(
        "target/{}",
        if release { "release" } else { "debug" }
    ));
    // Run cargo run
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if local {
        cmd.env("GFAAS_LOCAL", "");
    }
    cmd
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
