use gfaas::remote_fn;
use futures::stream::{self, StreamExt};

#[remote_fn(
    datadir = "/Users/kubkon/dev/yagna/ya-req",
    budget = 100,
)]
fn partial_sum(r#in: Vec<u64>) -> u64 {
    r#in.into_iter().sum()
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    let input: Vec<u64> = (0..100).collect();
    let input: Vec<_> = input.chunks(50).collect();
    let input = stream::iter(input);

    let output = input.fold(0u64, |acc, x| async move {
        let out = partial_sum(x.to_vec()).await;
        acc + out
    });
    let output = output.await?;
    println!("{:?}", output);
    assert_eq!((0..100).sum::<u64>(), output);
}
