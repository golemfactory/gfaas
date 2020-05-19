use gfaas::remote_fn;
use futures::stream::{self, StreamExt};

#[remote_fn(
    datadir = "/Users/kubkon/dev/datadir0",
    rpc_address = "127.0.0.1",
    rpc_port = 61000,
    net = "testnet"
)]
fn partial_sum(r#in: &[u8]) -> Vec<u8> {
    println!("Hit!");
    Vec::new()
}

#[actix_rt::main]
async fn main() {
    let input: Vec<u64> = (0..100).collect();
    let input: Vec<_> = input.chunks(10).collect();
    let input = stream::iter(input);

    let output = input.fold(Vec::new(), |mut acc, x| async move {
        acc.push(partial_sum(&[0,1,2]).await);
        acc
    });
    let output: Vec<_> = output.await;
    println!("{:?}", output);

    // assert_eq!((0..100).sum::<u64>(), output);
}
