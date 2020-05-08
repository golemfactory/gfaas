use gfaas::remote_fn;

#[remote_fn(
    datadir = "/Users/kubkon/dev/datadir0",
    rpc_address = "127.0.0.1",
    rpc_port = 61000,
    net = "testnet"
)]
pub fn hello(r#in: &[u8]) -> Vec<u8> {
    println!("START");
    let s = std::str::from_utf8(r#in).unwrap().to_uppercase();
    println!("STOP");
    s.as_bytes().to_vec()
}

#[actix_rt::main]
async fn main() {
    let r#in = "hey there gwasm";
    let out = hello(r#in.as_bytes()).await;
    println!("in: {}, out: {}", r#in, String::from_utf8(out).unwrap());
}
