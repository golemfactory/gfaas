use futures::future::{try_join_all, TryFutureExt};
use gfaas::remote_fn;
use std::{io::BufWriter, fs::File};

#[remote_fn]
fn compute_rectangle(start_y: u32, end_y: u32) -> Vec<u32> {
    use num_complex::Complex;

    const WIDTH: u32 = 600;
    const HEIGHT: u32 = 400;
    const RE_START: f64 = -2.0;
    const RE_END: f64 = 1.0;
    const IM_START: f64 = -1.0;
    const IM_END: f64 = 1.0;

    fn mandelbrot(c: Complex<f64>) -> u32 {
        const MAX_ITER: u32 = 80;

        let mut z = Complex::<f64>::default();
        let mut niter = 0;

        while z.norm() <= 2.0 && niter < MAX_ITER {
            z = z * z + c;
            niter += 1;
        }

        return niter;
    }

    println!(
        "Computing ({}, {}), ({}, {})",
        0, start_y, WIDTH, end_y
    );
    let mut output = vec![];
    for y in start_y..end_y {
        for x in 0..WIDTH {
            let c = Complex::new(
                RE_START + (x as f64 / WIDTH as f64) * (RE_END - RE_START),
                IM_START + (y as f64 / HEIGHT as f64) * (IM_END - IM_START),
            );
            output.push(mandelbrot(c));
        }
    }
    output
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    const WIDTH: u32 = 600;
    const HEIGHT: u32 = 400;
    const NUM: u32 = 4;

    let mut futures = vec![];
    for n in 0..NUM {
        let start_y = n * HEIGHT / NUM;
        let end_y = start_y + HEIGHT / NUM;
        futures.push(compute_rectangle(start_y, end_y).map_ok(move |x| (n, x)));
    }

    let mut output = try_join_all(futures).await?;
    let file = File::create("mandelbrot.png")?;
    let mut w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(&mut w, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    output.sort_by(|(x1, _), (x2, _)| x1.cmp(x2));
    let output: Vec<_> = output.into_iter().map(|(_, c)| c).flatten().map(|c| 255 - c as u8).collect();
    writer.write_image_data(&output)?;

    Ok(())
}
