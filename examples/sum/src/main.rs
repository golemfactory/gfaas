use gfaas::remote_fn;
use futures::stream::{self, StreamExt};

#[remote_fn(
    datadir = "/Users/kubkon/dev/yagna/ya-req",
    rpc_address = "127.0.0.1",
    rpc_port = 61000,
    net = "testnet"
)]
fn partial_sum(r#in: &[u8]) -> Vec<u8> {
    let s: Vec<u64> = serde_json::from_slice(r#in).unwrap();
    let s: u64 = s.into_iter().sum();
    serde_json::to_vec(&s).unwrap()
}

#[actix_rt::main]
async fn main() {
    let input: Vec<u64> = (0..100).collect();
    let input: Vec<_> = input.chunks(10).collect();
    let input = stream::iter(input);

    let output = input.fold(0u64, |acc, x| async move {
        let x = serde_json::to_vec(&x).unwrap();
        let out: Vec<u8> = partial_sum(&x).await;
        let out: u64 = serde_json::from_slice(&out).unwrap();
        acc + out
    });
    let output = output.await;
    println!("{:?}", output);
    assert_eq!((0..100).sum::<u64>(), output);
}
