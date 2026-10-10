//! Dev tool: make a "real world" version of a clean image (resample + JPEG round trip).
//! usage: degrade <in.png> <out.png> <scale_percent> <jpeg_quality>
use image::{imageops::FilterType, DynamicImage};
use std::io::Cursor;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = image::open(&a[1]).unwrap().to_rgb8();
    let pct: f64 = a[3].parse().unwrap();
    let q: u8 = a[4].parse().unwrap();
    let (w, h) = (img.width(), img.height());
    // shrink then enlarge back: soft edges like a low-res web logo scaled up
    let small = image::imageops::resize(
        &img,
        (w as f64 * pct / 100.0) as u32,
        (h as f64 * pct / 100.0) as u32,
        FilterType::Triangle,
    );
    let big = image::imageops::resize(&small, w, h, FilterType::CatmullRom);
    let mut buf = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut buf), q);
    DynamicImage::ImageRgb8(big)
        .write_with_encoder(enc)
        .unwrap();
    image::load_from_memory(&buf).unwrap().save(&a[2]).unwrap();
}
