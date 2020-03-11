use remote_fn::remote_fn;

#[remote_fn]
pub fn hello(r#in: &[u8]) -> Vec<u8> {
    use std::io::Write;
    use std::str;
    println!("START");
    let s = str::from_utf8(r#in).expect("valid UTF-8");
    println!("{:?}", s);
    println!("STOP");
    s.to_uppercase().as_bytes().to_vec()
}

fn main() {
    let r#in = "hey there gwasm!";
    let out = hello(r#in.as_bytes());
    let out = String::from_utf8(out).expect("valid UTF-8");
    println!("In: {}, Out: {}", r#in, out)
}
