//! Resolve anti-aliasing / blur blend pixels to the two colours they actually mix.
//!
//! Nearest-centroid assignment is only right for pixels close to a palette colour. A pixel
//! half-way between white and blue is far from both and may be nearest to an unrelated third
//! colour, which paints slivers along the edge. Such "uncertain" pixels are instead explained
//! as a mix of two colours that are present among their confident neighbours, and labelled
//! with the dominant one.

use super::{lab_dist2, Lab};
use rayon::prelude::*;

/// Neighbourhood radius (px) searched for confident pixels. Covers the width of a blurred edge.
const RADIUS: isize = 4;

pub fn resolve_blends(
    lab: &[Lab],
    opaque: &[bool],
    labels: &mut [u16],
    palette: &[Lab],
    w: usize,
    h: usize,
    tau: f32,
) {
    let tau2 = tau * tau;
    let snapshot: Vec<u16> = labels.to_vec();
    let confident: Vec<bool> = (0..w * h)
        .into_par_iter()
        .map(|i| opaque[i] && lab_dist2(&lab[i], &palette[snapshot[i] as usize]) <= tau2)
        .collect();

    labels.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let mut cands: [u16; 8] = [0; 8];
        for (x, out) in row.iter_mut().enumerate() {
            let i = y * w + x;
            if !opaque[i] || confident[i] {
                continue;
            }
            let mut n = 0usize;
            'scan: for dy in -RADIUS..=RADIUS {
                let yy = y as isize + dy;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                for dx in -RADIUS..=RADIUS {
                    let xx = x as isize + dx;
                    if xx < 0 || xx >= w as isize {
                        continue;
                    }
                    let j = yy as usize * w + xx as usize;
                    if !confident[j] {
                        continue;
                    }
                    let l = snapshot[j];
                    if !cands[..n].contains(&l) {
                        cands[n] = l;
                        n += 1;
                        if n == cands.len() {
                            break 'scan;
                        }
                    }
                }
            }
            match n {
                0 => {}
                1 => *out = cands[0],
                _ => *out = best_of_pairs(&lab[i], &cands[..n], palette),
            }
        }
    });
}

/// Explain `p` as a mixture of two candidate colours (closest segment) and return the
/// candidate with the larger share.
fn best_of_pairs(p: &Lab, cands: &[u16], palette: &[Lab]) -> u16 {
    let mut best = (f32::MAX, cands[0]);
    for (ia, &a) in cands.iter().enumerate() {
        let ca = &palette[a as usize];
        // Pure colour (t = 0).
        let d0 = lab_dist2(p, ca);
        if d0 < best.0 {
            best = (d0, a);
        }
        for &b in &cands[ia + 1..] {
            let cb = &palette[b as usize];
            let ab = [cb[0] - ca[0], cb[1] - ca[1], cb[2] - ca[2]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
            if len2 < 1e-9 {
                continue;
            }
            let t = (((p[0] - ca[0]) * ab[0] + (p[1] - ca[1]) * ab[1] + (p[2] - ca[2]) * ab[2])
                / len2)
                .clamp(0.0, 1.0);
            let q = [ca[0] + ab[0] * t, ca[1] + ab[1] * t, ca[2] + ab[2] * t];
            let d = lab_dist2(p, &q);
            // Prefer mixtures slightly over pure colours so a genuine blend is not snapped
            // to an arbitrary endpoint chosen only by noise.
            if d < best.0 {
                best = (d, if t >= 0.5 { b } else { a });
            }
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_pixel_goes_to_its_mixing_partner_not_a_third_colour() {
        // white | blend | blue, with an unrelated green palette entry closest to the blend
        let white = [1.0f32, 0.0, 0.0];
        let blue = [0.45f32, -0.03, -0.2];
        let green = [0.74f32, -0.04, -0.07];
        let blend = [0.72f32, -0.02, -0.12]; // ~ 1/2 white, 1/2 blue; nearest centroid is green
        let palette = vec![white, blue, green];
        let (w, h) = (9, 3);
        let mut lab = vec![white; w * h];
        for y in 0..h {
            for x in 5..w {
                lab[y * w + x] = blue;
            }
            lab[y * w + 4] = blend;
        }
        let mut labels: Vec<u16> = lab
            .iter()
            .map(|p| {
                (0..3)
                    .min_by(|&a, &b| {
                        lab_dist2(p, &palette[a])
                            .partial_cmp(&lab_dist2(p, &palette[b]))
                            .unwrap()
                    })
                    .unwrap() as u16
            })
            .collect();
        assert_eq!(labels[w + 4], 2, "setup: blend pixel is nearest to green");
        resolve_blends(&lab, &vec![true; w * h], &mut labels, &palette, w, h, 0.05);
        assert!(
            labels[w + 4] < 2,
            "blend must resolve to white or blue, not green"
        );
        assert!(labels.iter().all(|&l| l != 2));
    }
}
