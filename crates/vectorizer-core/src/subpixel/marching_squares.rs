//! Fractional marching squares on the label lattice.
//!
//! Classic marching squares thresholds a scalar field at 0.5 and linearly interpolates the
//! crossing between two sample centres: `P = p0 + (0.5 - v0)/(v1 - v0) · (p1 - p0)`.
//! Our field is the inverted anti-aliasing coverage α, which is an *area* integral, so the
//! exact crossing for a straight edge is obtained by summing coverage across the two pixels
//! that straddle the lattice edge (box-filter inversion):
//!
//! ```text
//!   offset = Σ α(pixel_i) - K     over K pixels on each side (α = coverage of label A)
//! ```
//!
//! For a hard (un-anti-aliased) edge `α = 1, 0` so the offset is `0` and the point sits on the
//! lattice-edge midpoint; for an anti-aliased or blurred edge it recovers the true sub-pixel
//! position. The sum agrees with the interpolation formula whenever α varies linearly, and
//! additionally conserves area, which keeps small shapes from shrinking or growing.

use super::CoverageField;
use crate::geom::Point;
use crate::topology::{LabelMap, OUTSIDE};

/// Exact iso-crossing along the segment `p0 → p1` (the textbook formula), kept public for
/// callers that sample a continuous scalar field.
#[inline]
pub fn iso_crossing(p0: Point, v0: f64, p1: Point, v1: f64, iso: f64) -> Point {
    if (v1 - v0).abs() < 1e-12 {
        return p0.lerp(p1, 0.5);
    }
    p0.lerp(p1, ((iso - v0) / (v1 - v0)).clamp(0.0, 1.0))
}

/// Sub-pixel boundary point for the unit lattice edge from lattice vertex `(x0, y0)` to
/// `(x1, y1)` (4-connected neighbours on the `(W+1)×(H+1)` corner lattice).
pub fn edge_point(
    field: &CoverageField,
    labels: &LabelMap,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
) -> Point {
    let mid = Point::new((x0 + x1) as f64 * 0.5, (y0 + y1) as f64 * 0.5);
    // Pixels on either side of the lattice edge, ordered (before, after) along the normal.
    let (pa, pb, normal) = if y0 == y1 {
        let x = x0.min(x1) as isize;
        let y = y0 as isize;
        ((x, y - 1), (x, y), Point::new(0.0, 1.0))
    } else {
        let x = x0 as isize;
        let y = y0.min(y1) as isize;
        ((x - 1, y), (x, y), Point::new(1.0, 0.0))
    };
    let la = labels.get(pa.0, pa.1);
    let lb = labels.get(pb.0, pb.1);
    if la == OUTSIDE || lb == OUTSIDE || la == lb {
        return mid; // the image border is an exact lattice line
    }
    // Integrate the coverage of `la` along the normal through the whole edge ramp: by area
    // conservation the boundary sits at `Σ α − K` pixels past the lattice edge, where the sum
    // runs over K pixels on either side. K = 1 is the box-filter case; a blurred or
    // re-sampled edge has a ramp several pixels wide, which K = RAMP covers.
    let (nx, ny) = (normal.x as isize, normal.y as isize);
    let mut sum = 0.0f32;
    for side in [-1isize, 0] {
        // side = -1: pixels before the edge (label la, ideal coverage 1);
        // side =  0: pixels after the edge  (label lb, ideal coverage 0).
        let ideal = if side < 0 { 1.0 } else { 0.0 };
        let mut stopped = false;
        for k in 0..RAMP as isize {
            let (px, py) = if side < 0 {
                (pa.0 - nx * k, pa.1 - ny * k)
            } else {
                (pb.0 + nx * k, pb.1 + ny * k)
            };
            let l = labels.get(px, py);
            if !stopped && (l == la || l == lb) {
                sum += field.coverage(px as usize, py as usize, la, lb);
            } else {
                // another region (or the canvas edge) ends the ramp: assume it is saturated
                stopped = true;
                sum += ideal;
            }
        }
    }
    if std::env::var("VZ_DEBUG").is_ok() && x0 == 352 && y0 == 200 && x1 == 352 {
        let cov: Vec<f32> = (-3isize..3)
            .map(|k| field.coverage((351 + k) as usize, 200, la, lb))
            .collect();
        eprintln!(
            "edge 352,200 la={la} lb={lb} cov={cov:?} sum={sum} pal={:?}",
            field.palette
        );
    }
    let d = (sum - RAMP as f32).clamp(-(RAMP as f32) + 0.5, RAMP as f32 - 0.5) as f64;
    mid + normal * d
}

/// Pixels integrated on each side of a boundary.
const RAMP: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_crossing_matches_formula() {
        let p = iso_crossing(Point::new(0.0, 0.0), 0.2, Point::new(1.0, 0.0), 0.8, 0.5);
        assert!((p.x - 0.5).abs() < 1e-12);
        let q = iso_crossing(Point::new(0.0, 0.0), 0.0, Point::new(4.0, 0.0), 1.0, 0.25);
        assert!((q.x - 1.0).abs() < 1e-12);
    }
}
