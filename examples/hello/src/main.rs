use gfaas::remote_fn;

#[remote_fn(budget = 100, timeout = 900, subnet = "devnet-alpha.2")]
pub fn hello(r#in: String) -> String {
    r#in.to_uppercase().to_string()
}

#[actix_rt::main]
async fn main() {
    pretty_env_logger::init();
    let r#in = "hey there gwasm";
    let out = match hello("hey there gwasm".to_string()).await {
        Ok(out) => out,
        Err(err) => {
            eprintln!("Unexpected error occurred: {}", err);
            return;
        }
    };
    println!("in: {}, out: {}", r#in, out);
}
