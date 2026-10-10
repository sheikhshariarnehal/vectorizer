//! Sub-pixel inversion of anti-aliased rasterisation.
//!
//! Every pixel on a region boundary is a convex mix `I = α·C₁ + (1-α)·C₂` of the two colours
//! meeting there. Recovering α (see [`distance_field`]) and integrating it across the
//! boundary ([`marching_squares`]) yields boundary positions with a precision far below one
//! pixel instead of the ±0.5 px staircase of a thresholded bitmap.
//!
//! The inversion is done in gamma-encoded sRGB, the space in which rasterisers blend edge
//! coverage; clustering uses Oklab, which is the right space for *perceptual* distance.

pub mod alpha_matte;
pub mod distance_field;
pub mod marching_squares;

use crate::topology::{LabelMap, OUTSIDE, TRANSPARENT};

/// Everything needed to estimate fractional coverage at a pixel.
pub struct CoverageField<'a> {
    pub width: usize,
    pub height: usize,
    /// Original straight RGBA8 pixels (un-filtered: a denoise filter would blur the AA ramp).
    pub rgba: &'a [u8],
    /// Palette colours as sRGB in 0..=255.
    pub palette: &'a [[f32; 3]],
}

impl CoverageField<'_> {
    /// Fractional coverage (0..=1) of label `a` at pixel `(x, y)`, assuming the pixel is a
    /// blend of labels `a` and `b`.
    #[inline]
    pub fn coverage(&self, x: usize, y: usize, a: u16, b: u16) -> f32 {
        let i = (y * self.width + x) * 4;
        let alpha = self.rgba[i + 3] as f32 / 255.0;
        if a == TRANSPARENT {
            return 1.0 - alpha;
        }
        if b == TRANSPARENT {
            return alpha;
        }
        debug_assert!(a != OUTSIDE && b != OUTSIDE);
        let px = [
            self.rgba[i] as f32,
            self.rgba[i + 1] as f32,
            self.rgba[i + 2] as f32,
        ];
        distance_field::blend_alpha(&px, &self.palette[a as usize], &self.palette[b as usize])
    }
}

/// Convenience used by tests and the pipeline.
pub fn make_labels(width: usize, height: usize, labels: Vec<u16>) -> LabelMap {
    LabelMap::new(width, height, labels)
}
