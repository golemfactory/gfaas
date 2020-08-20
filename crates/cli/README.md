# gfaas build tool

`gfaas` build tool is a wrapper of `cargo`. It is required to build Cargo projects which
aim at using the `gfaas` lib crate.

Since the tool is a thin wrapper around `cargo`, it can be used just like `cargo` would be.
Currently supported subcommands are: `build`, `run`, and `clean`.

For example, in order to build the project in release mode

```
gfaas build --release
```

or running

```
gfaas run --release
```
