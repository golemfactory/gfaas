# bellman

This app demonstrates how to write a zkSNARK app with the proof generation step offloaded to Golem
Network. The app uses [`bellman`](https://github.com/zkcrypto/bellman) crate to build the zkSNARK circuit.
In particular, the app implements proving the knowledge of some hash preimage.

## Usage

To build and run, you will need to download the current version of the `gfaas` build tool.
The tool is freely available in `crates.io`, and can be downloaded and installed as follows

```
cargo install gfaas-cli
```

### Building

To build the example,

```
gfaas build
```

### Running

The app features three steps: 1) generating public parameters, 2) generating proof, and 3) verifying
the generated proof. Steps 1) and 3) are computed locally, whereas step 2) is compute on Golem Network.

#### Generate public parameters

This step can only be run once per circuit. By default, the public parameters will be saved as a
whole to params file (you will need those params to generate the proof in the subsequent step).
The verification key will be saved separately to `vk` file (you will need the key to verify
the generated proof).

```
gfaas run -- generate-params
```

#### Generate proof

For this step, you will need the generated public parameters and some preimage. This step will
generate the public hash that you need to send together with the proof and verification key
to the proving party.

You can run this step locally (for testing),

```
GFAAS_RUN=local gfaas run -- generate-proof preimage
> DEADbeef123456
```

Or on Golem Network,

```
gfaas run -- generate-proof preimage
> DEADbeef123456
```

#### Verify proof

For this step, you will need the verification key, the proof, and the public hash of (a hash)
of the preimage.

```
gfaas run -- verify-proof DEADbeef123456
```
