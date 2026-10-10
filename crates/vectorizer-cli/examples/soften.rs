//! Dev tool: write a synthetic scene (optionally softened) to PNG.
//! usage: soften <scene> <w> <h> <blur_radius> <noise> <out.png>
use vectorizer_core::testing::*;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let img = match a[1].as_str() {
        "flag" => scene_flag(w, h),
        "ring" => scene_circle_ring(w),
        "rr" => scene_rounded_rect(w),
        "blob" => scene_blob(w),
        other => panic!("unknown scene {other}"),
    };
    let img = soften(&img, a[4].parse().unwrap(), a[5].parse().unwrap());
    image::RgbaImage::from_raw(img.width, img.height, img.data)
        .unwrap()
        .save(&a[6])
        .unwrap();
}
