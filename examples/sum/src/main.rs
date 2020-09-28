use futures::{
    lock::Mutex,
    stream::{self, TryStreamExt},
};
use gfaas::remote_fn;
use std::sync::Arc;

#[remote_fn(datadir = "/Users/kubkon/dev/yagna/ya-req", budget = 100)]
fn partial_sum(r#in: Vec<u64>) -> u64 {
    r#in.into_iter().sum()
}

const MAX_CONCURRENT_JOBS: usize = 1; // this is fixed in yarapi >= 0.2

#[actix_rt::main]
async fn main() {
    let input: Vec<u64> = (0..100).collect();
    let input = stream::iter(input.chunks(10).map(Ok));
    let sums = Arc::new(Mutex::new(Vec::new()));

    let fut = input.try_for_each_concurrent(MAX_CONCURRENT_JOBS, |x| {
        let sums = Arc::clone(&sums);
        async move {
            let sum = partial_sum(x.to_vec()).await?;
            sums.lock().await.push(sum);
            Ok(())
        }
    });

    let res: Result<_, gfaas::Error> = fut.await;
    if let Err(err) = res {
        eprintln!("Unexpected error occurred {}", err);
        return;
    }

    let sums =
        Arc::try_unwrap(sums).expect("container with partial sums should be computed by now");
    let sums = sums.into_inner();
    let final_sum = sums.into_iter().fold(0u64, |acc, x| acc + x);
    assert_eq!((0..100).sum::<u64>(), final_sum);
    println!("Calculated sum: {}", final_sum);
}
