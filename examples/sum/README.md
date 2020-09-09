# sum

This simple app demonstrates how to write a remote function which will compute a partial sum
of some slice of integers which will then be merged into a final result locally on your compute.
You can think of this app as a simple showcase of map-reduce style paradigm using `gfaas`.

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
