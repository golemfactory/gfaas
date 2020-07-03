# How to send Wasm binary directly to Yagna provider (not via Yagna app store)

This short guide explains step-by-step how to setup your Yagna requestor to be
able to deploy your custom Wasm binary at the selected provider. [Yagna] is the
codename for the Golem's Next Milestone.

[Yagna]: https://github.com/golemfactory/yagna

## 0. Setup your Yagna local cluster

This guide assumes you've already followed the main Yagna tutorial. If not,
head to [golemfactory/yagna/agent/provider/readme.md] and make sure you have followed
all of it and that everything works OK for you.

[golemfactory/yagna/agent/provider/readme.md]: https://github.com/golemfactory/yagna/blob/master/agent/provider/readme.md

## 1. Create your local "workspace"

Next, you'll need to create some local workspace where your requestor will serve
your custom Wasm binary from. You will also use this workspace to share any input/output
files with the provider.

Go ahead and create a workspace somewhere in your home dir. It'd probably be best
if you had it as a subdirectory of your requestor agent's main data folder. I'll assume
you've got the latter in `~/ya-req`. Then, go ahead and create `workspace`:

```
mkdir -p ~/ya-req/workspace
```

## 2. Create a valid Yagna Wasm package

Yagna expects packages to be zip archives consisting of your Wasm binaries (acting as
entrypoints), and a `manifest.json` file describing the contents.

For instance, suppose you were to package [`rust-wasi-tutorial`] as a Yagna compatible
package. Then, after building the Wasm module named `main.wasm`, you would put it in an archive, say
`custom.zip`, and you'd add the following manifest:

```json
{
  "id": "rust-wasi-tutorial",
  "name": "rust-wasi-tutorial",
  "entry-points": [
    {
      "id": "main",
      "wasm-path": "main.wasm"
    }
  ],
  "mount-points": [
    { "ro": "input" },
    { "wo": "output" }
  ]
}
```

Next, make sure to move the package `custom.zip` into your newly create local workspace:

```
mv custom.zip ~/ya-req/workspace
```

## 3 Create some dummy input

[`rust-wasi-tutorial`] Wasm binary expects two command line arguments: `in` and `out`. It will
then take the contents of `in` and simply copy it out to `out`.

Go ahead and create some dummy `in` file and move it to your local workspace:

```
echo "Hey there!" > in
mv in ~/ya-req/workspace
```

[`rust-wasi-tutorial`]: https://github.com/kubkon/rust-wasi-tutorial

## 4. Start up simple http server serving "workspace"

Enter your [`yagna`] project clone, and run:

[`yagna`]: https://github.com/golemfactory/yagna

```
cargo run --release -p ya-exe-unit --example http-get-put -- --root-dir /<path_to_home>/ya-req/workspace
```

This will now start an http server and allow you to serve Wasm binary and files between
your requestor node and providers.

## 5. Calculate the SHA3 hash of your package

Requesting computation of tasks in Yagna is currently facilitated via `ya-requstor` CLI (which is just
an example, with the intention being you'll create your own that suits your needs best). Now, every package
when launched in Golem needs to be checksumed. Therefore, let's calculate its SHA3 cause we'll need it later:

```
openssl dgst -sha3-512 ~/ya-req/workspace/custom.zip
```

## 6. Create a simple Yagna exe-script

In order to execute some commands, and more importantly, deploy the actual package, you need to create an
exe-script. For this guide, here's the simplest script that will deploy, start, transfer `in` to the provider,
run `main.wasm in out`, and finally transfer `out` back to the requestor, i.e., us:

```json
[
  {
    "deploy": {}
  },
  {
    "start":
    {
      "args": []
    }
  },
  {
    "transfer":
    {
      "from": "http://localhost:8000/in",
      "to": "container:/input/in"
    }
  },
  {
    "run":
    {
      "entry_point": "main",
      "args": ["/input/in", "/output/out"]
    }
  },
  {
    "transfer":
    {
      "from": "container:/output/out",
      "to": "http://localhost:8000/upload/out"
    }
  }
]
```

Let's call this script `simple.json`.

## 7. Run the script and serve your package!

Finally, we're now ready to serve our custom `custom.zip` package and execute `simple.json` exe-script. To
do this, run:

```
ya-requestor --task-package hash://sha3:the_hash_digest_from_step_5:http://localhost:8000/custom.zip --exe-script simple.json
```

After the provider finishes the work, you should find `out` file in `~/ya-req/workspace/`.

