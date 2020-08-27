use anyhow::{anyhow, bail, Context, Result};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str,
};
use structopt::{clap::AppSettings, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "gfaas",
    author = "Jakub Konka <kubkon@golem.network>",
    about = "Compile and run a Rust crate for the wasm32-wasi target for use in gWasm platform on Golem Network.",
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
    /// Run a local package
    Run {
        /// Run in release mode, with optimizations
        #[structopt(long)]
        release: bool,
        /// Pass additional arguments directly to cargo run command
        #[structopt()]
        args: Vec<String>,
    },
    /// Clean the build artefacts
    Clean {
        /// Pass additional arguments directly to cargo run command
        #[structopt()]
        args: Vec<String>,
    },
}

fn main() {
    let opt = Opt::from_args();
    let res = match opt.cmd {
        Subcommand::Build { release, args } => build(release, &args),
        Subcommand::Run { release, args } => run(release, &args),
        Subcommand::Clean { args } => clean(&args),
    };

    if let Err(err) = res {
        eprintln!("Unexpected error occurred: {}", err)
    }
}

fn build(release: bool, args: &Vec<String>) -> Result<()> {
    let profile = if release { "release" } else { "debug" };
    let out_dir = Path::new("target").join(&profile);

    // Fetch cargo manifest path for the root project
    let mut cmd = Command::new("cargo");
    let cmd_out = cmd
        .arg("metadata")
        .output()
        .context("running 'cargo metadata' command")?;
    let metadata: serde_json::Value = match serde_json::from_slice(&cmd_out.stdout) {
        Ok(metadata) => metadata,
        Err(_) => {
            // Spit out output from the `cargo metadata` as it might contain hints as to what
            // the error might be.
            let stderr = cmd_out.stderr;
            let stderr =
                str::from_utf8(&stderr).context("valid UTF8 in 'cargo metadata' output")?;
            eprintln!("{}", stderr);
            bail!("'cargo metadata' command failed");
        }
    };
    let workspace_root = metadata["workspace_root"]
        .as_str()
        .ok_or(anyhow!("metadata['workspace_root'] is not a UTF8 string"))?;

    // Create cargo package with gfaas funcs
    let module_path = out_dir.join("gfaas_modules");
    let bin_path = module_path.join("src").join("bin");
    if let Err(err) = fs::create_dir_all(&bin_path) {
        match err.kind() {
            io::ErrorKind::AlreadyExists => {}
            _ => bail!("couldn't create gfaas_module dir: {}", err),
        }
    }

    // Parse manifest of the workspace and extract gfaas deps
    let manifest_path = Path::new(workspace_root).join("Cargo.toml");
    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read '{}'", manifest_path.display()))?;
    let mut manifest_toml = contents
        .parse::<toml::Value>()
        .context("parsing contents of 'Cargo.toml' as TOML")?;
    let manifest_toml = manifest_toml
        .as_table_mut()
        .ok_or(anyhow!("malformed 'Cargo.toml'?"))?;

    let mut gfaas_toml = toml::toml! {
        [package]
        name = "gfaas_modules"
        version = "0.1.0"
    };

    if let Some(deps) = manifest_toml.remove("gfaas_dependencies") {
        gfaas_toml
            .as_table_mut()
            .unwrap()
            .insert("dependencies".to_owned(), deps.into());
    // TODO insert serde_json dep
    } else {
        gfaas_toml.as_table_mut().unwrap().insert(
            "dependencies".to_owned(),
            toml::toml! {
                serde_json = "1"
            },
        );
    }

    let gfaas_toml =
        toml::to_string(&gfaas_toml).context("couldn't serialize gfaas modules to TOML")?;
    fs::write(module_path.join("Cargo.toml"), gfaas_toml)
        .with_context(|| format!("saving '{}'", module_path.join("Cargo.toml").display()))?;

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
    let _cmd_out = cmd.output().context("failed to build the project")?;

    // Next, run cargo build --target=wasm32-wasi on gfaas_modules crate.
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--bins")
        .arg("--target=wasm32-wasi")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(&module_path);
    if release {
        cmd.arg("--release");
    }
    let _ = cmd.output().context("failed to build the gfaas modules")?;

    // Copy Wasm binaries next to the binary proper
    let from_dir = module_path
        .join("target")
        .join("wasm32-wasi")
        .join(&profile);
    let mut entries = vec![];
    for entry in fs::read_dir(&from_dir)? {
        let entry = entry?;
        let entry_path = entry.path();

        if let Some(ext) = entry_path.extension() {
            if ext == "wasm" {
                entries.push(PathBuf::from(entry_path.file_name().unwrap()));
            }
        }
    }

    if entries.is_empty() {
        bail!("no Wasm modules were generated!");
    }

    for entry in entries {
        let from_path = from_dir.join(&entry);
        let to_path = out_dir.join(entry);
        fs::copy(&from_path, &to_path).with_context(|| {
            format!(
                "copying final Wasm artifact to main output dir: '{}' -> '{}'",
                from_path.display(),
                to_path.display(),
            )
        })?;
    }

    Ok(())
}

fn run(release: bool, args: &Vec<String>) -> Result<()> {
    // We need to run cargo build first so that the Wasm artifacts are properly
    // generated.
    build(release, args)?;

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
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if release {
        cmd.arg("--release");
    }
    let _ = cmd.output().context("failed to run the project")?;

    Ok(())
}

fn clean(args: &Vec<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("clean")
        .args(args.iter().filter(|x| !x.contains("--target-dir")))
        .env("CARGO_TARGET_DIR", "target")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let _ = cmd.output().context("failed to clean the project")?;

    Ok(())
}
