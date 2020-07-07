# How to send Wasm binary directly to Yagna provider (not via Yagna app store)

This short guide explains step-by-step how to setup your Yagna requestor to be
able to deploy your custom Wasm binary at the selected provider. [Yagna] is the
codename for the Golem's Next Milestone.

[Yagna]: https://github.com/golemfactory/yagna

There are two backends we can use in order to distribute files between Yagna nodes:
via local HTTP service, or GFTP service.

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

## 4. Calculate the SHA3 hash of your package

Requesting computation of tasks in Yagna is currently facilitated via `ya-requstor` CLI (which is just
an example, with the intention being you'll create your own that suits your needs best). Now, every package
when launched in Golem needs to be checksumed. Therefore, let's calculate its SHA3 cause we'll need it later:

```
openssl dgst -sha3-512 ~/ya-req/workspace/custom.zip
```

## 5. Deploy and run

### 5.1. Using HTTP

Enter your [`yagna`] project clone, and run:

[`yagna`]: https://github.com/golemfactory/yagna

```
cargo run --release -p ya-exe-unit --example http-get-put -- --root-dir /<path_to_home>/ya-req/workspace
```

This will now start an http server and allow you to serve Wasm binary and files between
your requestor node and providers.

##### Create an exe-script

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

##### Execute the exe-script

Finally, we're now ready to serve our custom `custom.zip` package and execute `simple.json` exe-script. To
do this, run:

```
ya-requestor --task-package hash://sha3:the_hash_digest_from_step_4:http://localhost:8000/custom.zip --exe-script simple.json
```

After the provider finishes the work, you should find `out` file in `~/ya-req/workspace/`.

### 5.2. Using GFTP

Enter your [`yagna`] project clone, and run:

[`yagna`]: https://github.com/golemfactory/yagna

```
cargo run --release -p gftp -- server -v
```

This will now start a gftp server and allow you to share Wasm binary and files between
your requestor node and providers using the GFTP protocol.

When this is up and running, firstly, let's publish our `custom.zip` package and `workspace/in` input file. To
do so, paste this JSON command in the same terminal where you're running the gftp server:

```
{"jsonrpc": "2.0", "id": "1", "method": "publish", "params": {"files": ["workspace/custom.zip", "workspace/in"]}}
```

You should receive a response similar to the following one:

```
{"jsonrpc":"2.0","id":"1","result":[{"file":"workspace/custom.zip","url":"gftp://0xffca35777bce97be413784aa441dff008c1d1281/73930bc2830da7a562b73ca9dc402b4d07205a8aab7ba01fd4b7ea54cdc48015"},{"file":"workspace/in","url":"gftp://0xffca35777bce97be413784aa441dff008c1d1281/9b0203f4e67182274d787d0dc843f42d61fc2b4cfd066b88902ed58d7194faa2"}]}
```

Make sure you record the generated `url` links as you'll need them in your exe-script as well as when executing
`ya-requestor` CLI.

Next, we need to prepare an endpoint where we'd like to receive the result. This will be `workspace/out`:

```
{"jsonrpc": "2.0", "id": "3", "method": "receive", "params": {"output_file": "workspace/out"}}
```

You should receive a response similar to the following one:

```
{"jsonrpc":"2.0","id":"3","result":{"file":"workspace/out","url":"gftp://0xffca35777bce97be413784aa441dff008c1d1281/D9udsU3PwgpgGlLDjyXefOXPLYePyUSMBFmAPB42txyaT7TvoDxcAYZOugWx5W1IN"}}
```

##### Create an exe-script

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
      "from": "gftp://0xffca35777bce97be413784aa441dff008c1d1281/9b0203f4e67182274d787d0dc843f42d61fc2b4cfd066b88902ed58d7194faa2",
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
      "to": "gftp://0xffca35777bce97be413784aa441dff008c1d1281/D9udsU3PwgpgGlLDjyXefOXPLYePyUSMBFmAPB42txyaT7TvoDxcAYZOugWx5W1IN"
    }
  }
]
```

Let's call this script `simple.json`.

##### Execute the exe-script

Finally, we're now ready to serve our custom `custom.zip` package and execute `simple.json` exe-script. To
do this, run:

```
ya-requestor --task-package hash://sha3:the_hash_digest_from_step_4:gftp://0xffca35777bce97be413784aa441dff008c1d1281/73930bc2830da7a562b73ca9dc402b4d07205a8aab7ba01fd4b7ea54cdc48015 --exe-script simple.json
```

After the provider finishes the work, you should find `out` file in `~/ya-req/workspace/`.

