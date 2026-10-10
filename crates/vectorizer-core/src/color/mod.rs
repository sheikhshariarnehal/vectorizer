//! Perceptual colour handling: Oklab conversion, edge-preserving denoise, K-Means palette.

pub mod bilateral;
pub mod blend;
pub mod kmeans;
pub mod oklab;

pub type Lab = [f32; 3];

#[inline]
pub fn lab_dist2(a: &Lab, b: &Lab) -> f32 {
    let d0 = a[0] - b[0];
    let d1 = a[1] - b[1];
    let d2 = a[2] - b[2];
    d0 * d0 + d1 * d1 + d2 * d2
}
