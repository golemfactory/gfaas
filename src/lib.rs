pub mod __private {
    pub use tempfile;
    pub use tokio;
    pub use wasi_rt;

    #[allow(unused)]
    pub mod package {
        use anyhow::Result;
        use std::{
            fs,
            io::{Cursor, Write},
            path::Path,
        };
        use zip::{write::FileOptions, CompressionMethod, ZipWriter};

        pub struct Package {
            zip_writer: ZipWriter<Cursor<Vec<u8>>>,
            options: FileOptions,
            module_name: Option<String>,
        }

        impl Package {
            pub fn new() -> Self {
                let options = FileOptions::default().compression_method(CompressionMethod::Stored);
                let zip_writer = ZipWriter::new(Cursor::new(Vec::new()));

                Self {
                    zip_writer,
                    options,
                    module_name: None,
                }
            }

            pub fn add_module_from_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
                let module_name = path
                    .as_ref()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let contents = fs::read(path.as_ref())?;
                self.zip_writer
                    .start_file(&module_name, self.options.clone())?;
                self.zip_writer.write(&contents)?;
                self.module_name = Some(module_name);

                Ok(())
            }

            pub fn write<P: AsRef<Path>>(mut self, path: P) -> Result<()> {
                // create manifest
                let comps: Vec<_> = self.module_name.as_ref().unwrap().split('.').collect();
                let manifest = serde_json::json!({
                    "id": "custom",
                    "name": "custom",
                    "entry-points": [{
                        "id": comps[0],
                        "wasm-path": self.module_name.unwrap(),
                    }],
                    "mount-points": [{
                        "rw": "workdir",
                    }]
                });
                self.zip_writer
                    .start_file("manifest.json", self.options.clone())?;
                self.zip_writer.write(&serde_json::to_vec(&manifest)?)?;

                let finalized = self.zip_writer.finish()?.into_inner();
                fs::write(path.as_ref(), finalized)?;

                Ok(())
            }
        }
    }
}

pub use gfaas_macro::remote_fn;
