# gfaas

This is an experimental implementation of Function-as-a-Service (FaaS)
on top of [gWasm] in [Golem Network]. It's currently not-even-alpha-ready
so use it at your own risk! Or put it another way, things are expected
to break. If they don't, well, that's impossible, isn't it? ;-)

## Usage

### `gfaas` lib

In your `Cargo.toml`, put `gfaas` as your dependency

```
# Cargo.toml
[dependencies]
gfaas = { git = "https://github.com/kubkon/cuddly-jumpers" }
```

You can then annotate a function that accepts a byte slice, and
returns a byte `Vec` as `gfaas::remote_fn` which will make it gWasm and
Golem Network ready

```rust
#[remote_fn]
fn compute(r#in: &[u8]) -> Vec<u8> {
    // some logic...
}
```

Then, you can use `compute` as you normally would.

### `gfaas` build tool

One caveat here is that to compile your code with annotated remote functions
you should use the `gfaas` tool rather than bare `cargo`. `gfaas` is essentially
a wrapper for `cargo` orchestrating compilation to native as well as Emscripten Wasm
targets, all in one go.

The tool is available in [crates/cli](crates/cli). Use it as you would normally use
`cargo`:

```
$ gfaas build --release
$ gfaas run --release
$ gfaas clean
```

Note however that currently only those three functionalities are supported: `build`, `run`,
and `clean`. If you would like to pass more options/flags directly to `cargo`, you can
do it via `--` at the end of the invocation:

```
$ gfaas build --release -- --quiet
```
