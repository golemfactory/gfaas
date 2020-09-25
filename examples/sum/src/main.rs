use anyhow::Result;
use futures::stream::{self, TryStreamExt};
use gfaas::remote_fn;

#[remote_fn(datadir = "/Users/kubkon/dev/yagna/ya-req", budget = 100)]
fn partial_sum(r#in: Vec<u64>) -> u64 {
    r#in.into_iter().sum()
}

#[actix_rt::main]
async fn main() -> Result<()> {
    let input: Vec<u64> = (0..100).collect();
    let input: Vec<Result<_>> = input.chunks(10).map(|x| Ok(x)).collect();
    let input = stream::iter(input);

    let output = input.try_fold(0u64, |acc, x| async move {
        let out = partial_sum(x.to_vec()).await?;
        Ok(acc + out)
    });
    let output = output.await?;
    println!("{:?}", output);
    assert_eq!((0..100).sum::<u64>(), output);

    Ok(())
}
