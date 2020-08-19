use gfaas::remote_fn;

#[remote_fn(
    datadir = "/Users/kubkon/dev/yagna/ya-req",
    budget = 100,
)]
pub fn hello(r#in: String) -> String {
    r#in.to_uppercase().to_string()
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    let r#in = "hey there gwasm";
    let out = hello("hey there gwasm".to_string()).await?;
    println!("in: {}, out: {}", r#in, out);

    Ok(())
}
