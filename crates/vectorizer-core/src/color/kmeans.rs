//! Deterministic, multi-threaded K-Means palette quantizer in Oklab space.

use super::oklab::{linear_to_oklab, linear_to_srgb_f, oklab_to_linear, srgb_to_linear_f};
use super::{lab_dist2, Lab};
use rayon::prelude::*;

const MAX_SAMPLES: usize = 65_536;
const CHUNK: usize = 2048;

struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[inline]
fn nearest(palette: &[Lab], p: &Lab) -> (usize, f32) {
    let mut best = 0;
    let mut bd = f32::MAX;
    for (i, c) in palette.iter().enumerate() {
        let d = lab_dist2(c, p);
        if d < bd {
            bd = d;
            best = i;
        }
    }
    (best, bd)
}

/// Cluster the `train` pixels into at most `k` colours.
/// Returns `(centroids, approximate pixel counts)`; empty clusters are dropped.
pub fn quantize(lab: &[Lab], train: &[bool], k: usize, iters: usize) -> (Vec<Lab>, Vec<f64>) {
    let n_train = train.iter().filter(|&&t| t).count();
    if n_train == 0 {
        return (Vec::new(), Vec::new());
    }
    let stride = (n_train / MAX_SAMPLES).max(1);
    let mut samples: Vec<Lab> = Vec::with_capacity(n_train / stride + 1);
    let mut seen = 0usize;
    for (i, &t) in train.iter().enumerate() {
        if t {
            if seen % stride == 0 {
                samples.push(lab[i]);
            }
            seen += 1;
        }
    }
    let k = k.min(samples.len()).max(1);

    // --- k-means++ seeding ---------------------------------------------------------------
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut centroids: Vec<Lab> = Vec::with_capacity(k);
    centroids.push(samples[(rng.next_f64() * samples.len() as f64) as usize % samples.len()]);
    let mut d2: Vec<f32> = samples
        .iter()
        .map(|s| lab_dist2(s, &centroids[0]))
        .collect();
    while centroids.len() < k {
        let total: f64 = d2.iter().map(|&d| d as f64).sum();
        if total <= 1e-12 {
            break; // every sample coincides with an existing centroid
        }
        let mut target = rng.next_f64() * total;
        let mut pick = samples.len() - 1;
        for (i, &d) in d2.iter().enumerate() {
            target -= d as f64;
            if target <= 0.0 {
                pick = i;
                break;
            }
        }
        let c = samples[pick];
        centroids.push(c);
        d2.par_iter_mut()
            .zip(samples.par_iter())
            .for_each(|(d, s)| {
                let nd = lab_dist2(s, &c);
                if nd < *d {
                    *d = nd;
                }
            });
    }

    // --- Lloyd iterations ------------------------------------------------------------------
    let kk = centroids.len();
    let mut counts = vec![0.0f64; kk];
    for _ in 0..iters {
        // Fixed chunking + ordered reduction keeps results deterministic across thread counts.
        let partials: Vec<Vec<[f64; 4]>> = samples
            .par_chunks(CHUNK)
            .map(|chunk| {
                let mut acc = vec![[0.0f64; 4]; kk];
                for s in chunk {
                    let (i, _) = nearest(&centroids, s);
                    let a = &mut acc[i];
                    a[0] += s[0] as f64;
                    a[1] += s[1] as f64;
                    a[2] += s[2] as f64;
                    a[3] += 1.0;
                }
                acc
            })
            .collect();
        let mut sums = vec![[0.0f64; 4]; kk];
        for p in &partials {
            for (s, a) in sums.iter_mut().zip(p) {
                for j in 0..4 {
                    s[j] += a[j];
                }
            }
        }
        let mut moved = 0.0f32;
        for (i, s) in sums.iter().enumerate() {
            counts[i] = s[3];
            if s[3] > 0.0 {
                let nc = [
                    (s[0] / s[3]) as f32,
                    (s[1] / s[3]) as f32,
                    (s[2] / s[3]) as f32,
                ];
                moved = moved.max(lab_dist2(&nc, &centroids[i]));
                centroids[i] = nc;
            }
        }
        if moved < 1e-9 {
            break;
        }
    }

    let mut pal = Vec::new();
    let mut cnt = Vec::new();
    for (c, &n) in centroids.iter().zip(&counts) {
        if n > 0.0 {
            pal.push(*c);
            cnt.push(n * stride as f64);
        }
    }
    (pal, cnt)
}

/// Merge near-identical entries and (optionally) drop entries that are just anti-aliasing
/// blends lying on the straight line between two more frequent colours.
pub fn refine_palette(mut pal: Vec<Lab>, mut cnt: Vec<f64>, prune_blends: bool) -> Vec<Lab> {
    const MERGE_DIST2: f32 = 0.015 * 0.015;
    // Merge duplicates.
    let mut i = 0;
    while i < pal.len() {
        let mut j = i + 1;
        while j < pal.len() {
            if lab_dist2(&pal[i], &pal[j]) < MERGE_DIST2 {
                let (wi, wj) = (cnt[i].max(1e-9), cnt[j].max(1e-9));
                let t = (wj / (wi + wj)) as f32;
                for c in 0..3 {
                    pal[i][c] += (pal[j][c] - pal[i][c]) * t;
                }
                cnt[i] += cnt[j];
                pal.swap_remove(j);
                cnt.swap_remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    if !prune_blends || pal.len() < 3 {
        return pal;
    }

    const BLEND_DIST: f32 = 0.035;
    let lin: Vec<[f32; 3]> = pal.iter().map(|&c| oklab_to_linear(c)).collect();
    let mut order: Vec<usize> = (0..pal.len()).collect();
    order.sort_by(|&a, &b| {
        cnt[a]
            .partial_cmp(&cnt[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut removed = vec![false; pal.len()];
    for &c in &order {
        if pal.len() - removed.iter().filter(|&&r| r).count() <= 2 {
            break;
        }
        let pc = pal[c];
        let mut is_blend = false;
        'outer: for a in 0..pal.len() {
            if a == c || removed[a] || cnt[a] <= cnt[c] {
                continue;
            }
            for b in (a + 1)..pal.len() {
                if b == c || removed[b] || cnt[b] <= cnt[c] {
                    continue;
                }
                let ab = [
                    pal[b][0] - pal[a][0],
                    pal[b][1] - pal[a][1],
                    pal[b][2] - pal[a][2],
                ];
                let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                if len2 < 1e-6 {
                    continue;
                }
                let ac = [pc[0] - pal[a][0], pc[1] - pal[a][1], pc[2] - pal[a][2]];
                let t = (ac[0] * ab[0] + ac[1] * ab[1] + ac[2] * ab[2]) / len2;
                if !(-0.02..=1.02).contains(&t) {
                    continue;
                }
                let proj = [
                    pal[a][0] + ab[0] * t,
                    pal[a][1] + ab[1] * t,
                    pal[a][2] + ab[2] * t,
                ];
                let chord2 = lab_dist2(&proj, &pc);
                if chord2 < BLEND_DIST * BLEND_DIST {
                    is_blend = true;
                    break 'outer;
                }
                // Rasterisers blend in gamma-encoded or linear RGB, whose path through
                // Oklab bends away from the straight chord.
                if chord2 < 0.12 * 0.12 && blends_nonlinear(&lin[a], &lin[b], &pc) {
                    is_blend = true;
                    break 'outer;
                }
            }
        }
        if is_blend {
            removed[c] = true;
        }
    }
    pal.into_iter()
        .zip(removed)
        .filter(|(_, r)| !*r)
        .map(|(c, _)| c)
        .collect()
}

/// True if `c` lies close to the path `la → lb` when the two (linear-light) colours are mixed
/// either in gamma-encoded sRGB or in linear light.
fn blends_nonlinear(la: &[f32; 3], lb: &[f32; 3], c: &Lab) -> bool {
    const TOL2: f32 = 0.03 * 0.03;
    let ga: [f32; 3] = [
        linear_to_srgb_f(la[0]),
        linear_to_srgb_f(la[1]),
        linear_to_srgb_f(la[2]),
    ];
    let gb: [f32; 3] = [
        linear_to_srgb_f(lb[0]),
        linear_to_srgb_f(lb[1]),
        linear_to_srgb_f(lb[2]),
    ];
    for k in 1..20 {
        let t = k as f32 * 0.05;
        let g = [
            srgb_to_linear_f(ga[0] + (gb[0] - ga[0]) * t),
            srgb_to_linear_f(ga[1] + (gb[1] - ga[1]) * t),
            srgb_to_linear_f(ga[2] + (gb[2] - ga[2]) * t),
        ];
        if lab_dist2(&linear_to_oklab(g), c) < TOL2 {
            return true;
        }
        let l = [
            la[0] + (lb[0] - la[0]) * t,
            la[1] + (lb[1] - la[1]) * t,
            la[2] + (lb[2] - la[2]) * t,
        ];
        if lab_dist2(&linear_to_oklab(l), c) < TOL2 {
            return true;
        }
    }
    false
}

/// Assign every `opaque` pixel to its nearest palette entry; others get `transparent`.
pub fn assign(lab: &[Lab], opaque: &[bool], palette: &[Lab], transparent: u16) -> Vec<u16> {
    lab.par_iter()
        .zip(opaque.par_iter())
        .map(|(p, &o)| {
            if o {
                nearest(palette, p).0 as u16
            } else {
                transparent
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_three_flat_colours() {
        let cols = [[0.2f32, 0.0, 0.0], [0.6, 0.1, 0.1], [0.9, -0.05, 0.05]];
        let lab: Vec<Lab> = (0..30_000).map(|i| cols[i % 3]).collect();
        let (pal, cnt) = quantize(&lab, &vec![true; lab.len()], 8, 10);
        let pal = refine_palette(pal, cnt, true);
        assert_eq!(pal.len(), 3);
    }

    #[test]
    fn prunes_gamma_space_blend() {
        use crate::color::oklab::srgb_to_oklab;
        let a = srgb_to_oklab(220, 30, 25);
        let b = srgb_to_oklab(25, 64, 190);
        let mid = srgb_to_oklab(122, 47, 108); // 50 % mix computed on the sRGB values
        let pal = refine_palette(vec![a, b, mid], vec![1000.0, 1000.0, 40.0], true);
        assert_eq!(pal.len(), 2);
        // ...but a genuinely different third colour survives.
        let other = srgb_to_oklab(30, 170, 60);
        let pal = refine_palette(vec![a, b, other], vec![1000.0, 1000.0, 40.0], true);
        assert_eq!(pal.len(), 3);
    }

    #[test]
    fn prunes_blend_colour() {
        let a = [0.1f32, 0.0, 0.0];
        let b = [0.9f32, 0.1, 0.0];
        let mid = [0.5f32, 0.05, 0.0];
        let pal = refine_palette(vec![a, b, mid], vec![1000.0, 1000.0, 50.0], true);
        assert_eq!(pal.len(), 2);
    }
}
