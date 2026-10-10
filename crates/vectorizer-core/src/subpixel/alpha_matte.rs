//! Alpha handling: opacity mask and removal of colour fringes around transparent cut-outs.

use crate::types::RasterImage;

/// Pixels with alpha ≥ `threshold` are part of the artwork.
pub fn opaque_mask(img: &RasterImage, threshold: u8) -> Vec<bool> {
    img.data
        .chunks_exact(4)
        .map(|p| p[3] >= threshold.max(1))
        .collect()
}

/// Pixels that are (almost) fully opaque: their colour is trustworthy for palette training.
pub fn solid_mask(img: &RasterImage) -> Vec<bool> {
    img.data.chunks_exact(4).map(|p| p[3] >= 250).collect()
}

/// Semi-transparent pixels on the rim of a cut-out carry unreliable colours (matte bleed,
/// premultiplication residue). Give them the label of a fully opaque neighbour so the
/// artwork never grows a dark or light outline. The *position* of the rim still comes from
/// the alpha channel itself.
pub fn heal_fringe(labels: &mut [u16], img: &RasterImage, solid: &[bool], opaque: &[bool]) {
    let (w, h) = (img.width as usize, img.height as usize);
    let snapshot = labels.to_vec();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !opaque[i] || solid[i] {
                continue;
            }
            // Majority vote among solid 8-neighbours.
            let mut votes: [(u16, u8); 8] = [(0, 0); 8];
            let mut nv = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                        continue;
                    }
                    let j = yy as usize * w + xx as usize;
                    if !solid[j] {
                        continue;
                    }
                    let l = snapshot[j];
                    if let Some(v) = votes[..nv].iter_mut().find(|v| v.0 == l) {
                        v.1 += 1;
                    } else {
                        votes[nv] = (l, 1);
                        nv += 1;
                    }
                }
            }
            if let Some(best) = votes[..nv].iter().max_by_key(|v| v.1) {
                labels[i] = best.0;
            }
        }
    }
}
