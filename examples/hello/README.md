# hello

This "Hello world!" style app demonstrates how to write a simple remote function
which takes a `String` as input and returns a `String` as output.

## Usage

To build and run, you will need to download the current version of the `gfaas` build tool.
The tool is freely available in `crates.io`, and can be downloaded and installed as follows

```
cargo install gfaas-cli
```

To build the example,

```
gfaas build
```

To run locally,

```
GFAAS_RUN=local gfaas run
```

To run on Golem Network,

```
gfaas run
```
