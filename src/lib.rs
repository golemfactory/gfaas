pub mod __private {
    pub use anyhow;
    pub use serde_json;
    pub use tempfile;
    pub use tokio;
    pub use wasi_rt;
    pub use zip;
}

pub use gfaas_macro::remote_fn;
