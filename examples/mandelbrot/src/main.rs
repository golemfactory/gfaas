use anyhow::Result;
use futures::{
    lock::Mutex,
    stream::{self, TryStreamExt},
};
use gfaas::remote_fn;
use std::{fs::File, io::BufWriter, sync::Arc};
use structopt::StructOpt;

#[remote_fn(budget = 1000, timeout = 900, subnet = "devnet-alpha.2")]
fn compute_rectangle(start_y: u32, end_y: u32, width: u32, height: u32) -> Vec<u32> {
    use num_complex::Complex;

    const RE_START: f64 = -2.0;
    const RE_END: f64 = 1.0;
    const IM_START: f64 = -1.0;
    const IM_END: f64 = 1.0;

    fn mandelbrot(c: Complex<f64>) -> u32 {
        const MAX_ITER: u32 = 255;

        let mut z = Complex::<f64>::default();
        let mut niter = 0;

        while z.norm() <= 2.0 && niter < MAX_ITER {
            z = z * z + c;
            niter += 1;
        }

        return niter;
    }

    let mut output = vec![];
    for y in start_y..end_y {
        for x in 0..width {
            let c = Complex::new(
                RE_START + (x as f64 / width as f64) * (RE_END - RE_START),
                IM_START + (y as f64 / height as f64) * (IM_END - IM_START),
            );
            output.push(mandelbrot(c));
        }
    }
    output
}

#[derive(StructOpt)]
#[structopt(
    name = "mandelbrot",
    about = "Generates a Mandelbrot set and saves as PNG."
)]
struct Opt {
    /// Width of the image to generate.
    #[structopt(short, long, default_value = "600")]
    width: u32,

    /// Height of the image to generate.
    #[structopt(short, long, default_value = "400")]
    height: u32,

    /// Maximum number of parallel computations to run.
    #[structopt(short, long, default_value = "4")]
    in_parallel: u32,
}

const MAX_CONCURRENT_JOBS: usize = 4;

#[actix_rt::main]
async fn main() -> Result<()> {
    const MAX_ITER: u32 = 255;

    let opts = Opt::from_args();

    let max_row_size = (opts.height as f64 / opts.in_parallel as f64).ceil() as u32;
    let width = opts.width;
    let height = opts.height;

    let mut chunks: Vec<Result<_>> = vec![];
    for n in 0..opts.in_parallel {
        let start_y = n * max_row_size;
        let end_y = if start_y + max_row_size > height {
            height
        } else {
            start_y + max_row_size
        };
        chunks.push(Ok((n, start_y, end_y)));
    }

    let output = Arc::new(Mutex::new(Vec::new()));
    let chunks = stream::iter(chunks);
    chunks
        .try_for_each_concurrent(MAX_CONCURRENT_JOBS, |(n, start_y, end_y)| {
            let output = Arc::clone(&output);
            async move {
                let rect = compute_rectangle(start_y, end_y, width, height).await?;
                output.lock().await.push((n, rect));
                Ok(())
            }
        })
        .await?;

    let file = File::create("mandelbrot.png")?;
    let mut w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(&mut w, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    let output = Arc::try_unwrap(output)
        .expect("container with computation results should be fully filled in by now");
    let mut output = output.into_inner();
    output.sort_by(|(x1, _), (x2, _)| x1.cmp(x2));
    let output: Vec<_> = output
        .into_iter()
        .map(|(_, c)| c)
        .flatten()
        .map(|c| (MAX_ITER - c) as u8)
        .collect();
    writer.write_image_data(&output)?;

    Ok(())
}
