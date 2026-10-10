//! Edge-preserving bilateral filter in Oklab space.

use super::{lab_dist2, Lab};
use rayon::prelude::*;

/// Filter `src` (row-major `w*h`). Pixels where `opaque[i]` is false neither contribute
/// to nor are modified by the filter.
pub fn bilateral(src: &[Lab], opaque: &[bool], w: usize, h: usize, radius: usize) -> Vec<Lab> {
    if radius == 0 {
        return src.to_vec();
    }
    let r = radius as isize;
    let sigma_s = (radius as f32 * 0.8).max(0.8);
    // Range sigma in Oklab units: small enough to leave real edges (ΔE >> 0.1) untouched.
    let sigma_r = 0.05f32;
    let inv2s2 = 1.0 / (2.0 * sigma_s * sigma_s);
    let cutoff2 = 9.0 * sigma_r * sigma_r; // beyond 3σ the weight is negligible
    const BINS: usize = 128;
    let range_lut: Vec<f32> = (0..BINS)
        .map(|i| {
            let d2 = (i as f32 + 0.5) / BINS as f32 * cutoff2;
            (-d2 / (2.0 * sigma_r * sigma_r)).exp()
        })
        .collect();
    let side = (2 * radius + 1) as usize;
    let mut spatial = vec![0.0f32; side * side];
    for dy in -r..=r {
        for dx in -r..=r {
            spatial[(dy + r) as usize * side + (dx + r) as usize] =
                (-((dx * dx + dy * dy) as f32) * inv2s2).exp();
        }
    }

    let mut out = vec![[0.0f32; 3]; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let c = src[i];
            if !opaque[i] {
                *o = c;
                continue;
            }
            let mut acc = [0.0f32; 3];
            let mut wsum = 0.0f32;
            for dy in -r..=r {
                let yy = y as isize + dy;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                for dx in -r..=r {
                    let xx = x as isize + dx;
                    if xx < 0 || xx >= w as isize {
                        continue;
                    }
                    let j = yy as usize * w + xx as usize;
                    if !opaque[j] {
                        continue;
                    }
                    let n = &src[j];
                    let d2 = lab_dist2(&c, n);
                    if d2 >= cutoff2 {
                        continue;
                    }
                    let bin = ((d2 / cutoff2) * BINS as f32) as usize;
                    let wt = spatial[(dy + r) as usize * side + (dx + r) as usize]
                        * range_lut[bin.min(BINS - 1)];
                    acc[0] += n[0] * wt;
                    acc[1] += n[1] * wt;
                    acc[2] += n[2] * wt;
                    wsum += wt;
                }
            }
            *o = if wsum > 0.0 {
                [acc[0] / wsum, acc[1] / wsum, acc[2] / wsum]
            } else {
                c
            };
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_hard_edge_and_flattens_noise() {
        let (w, h) = (16, 8);
        let mut img = vec![[0.0f32; 3]; w * h];
        for y in 0..h {
            for x in 0..w {
                let base = if x < 8 { 0.2 } else { 0.9 };
                let noise = if (x + y) % 2 == 0 { 0.01 } else { -0.01 };
                img[y * w + x] = [base + noise, 0.0, 0.0];
            }
        }
        let out = bilateral(&img, &vec![true; w * h], w, h, 1);
        let left = out[4 * w + 3][0];
        let right = out[4 * w + 12][0];
        assert!((left - 0.2).abs() < 0.006);
        assert!((right - 0.9).abs() < 0.006);
        // The edge itself must stay sharp.
        assert!(out[4 * w + 7][0] < 0.3 && out[4 * w + 8][0] > 0.8);
    }
}
