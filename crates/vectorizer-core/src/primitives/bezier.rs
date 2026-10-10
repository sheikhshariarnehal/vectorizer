//! Schneider's adaptive cubic Bézier fitting ("An Algorithm for Automatically Fitting
//! Digitized Curves", Graphics Gems, 1990) with fixed end points.

use super::{cubic_point, Primitive};
use crate::geom::Point;

/// A fitted cubic together with the index range of the input points it covers.
#[derive(Clone, Copy, Debug)]
pub struct CubicPiece {
    pub prim: Primitive,
    pub lo: usize,
    pub hi: usize,
}

/// Fit `pts` with a chain of cubics so that every point is within `tol` of the curve.
///
/// `t_start` is the unit tangent leaving the first point; `t_end` the unit tangent at the
/// last point pointing *back into* the curve. When `None` they are estimated from the data.
pub fn fit_cubics(
    pts: &[Point],
    tol: f64,
    t_start: Option<Point>,
    t_end: Option<Point>,
) -> Vec<CubicPiece> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let k = 3.min(n - 1);
    let t1 = t_start.unwrap_or_else(|| tangent_or_chord(pts, 0, k, pts[n - 1] - pts[0]));
    let t2 = t_end.unwrap_or_else(|| tangent_or_chord(pts, n - 1, n - 1 - k, pts[0] - pts[n - 1]));
    let mut out = Vec::new();
    fit_rec(pts, 0, n - 1, t1, t2, tol, &mut out, 0);
    out
}

fn tangent_or_chord(pts: &[Point], from: usize, to: usize, fallback: Point) -> Point {
    let t = (pts[to] - pts[from]).normalized();
    if t == Point::ZERO {
        fallback.normalized()
    } else {
        t
    }
}

#[allow(clippy::too_many_arguments)]
fn fit_rec(
    d: &[Point],
    first: usize,
    last: usize,
    t1: Point,
    t2: Point,
    tol: f64,
    out: &mut Vec<CubicPiece>,
    depth: usize,
) {
    let n = last - first + 1;
    let (p0, p3) = (d[first], d[last]);
    if n == 2 {
        let dist = p0.dist(p3) / 3.0;
        out.push(CubicPiece {
            prim: Primitive::Cubic {
                a: p0,
                c1: p0 + t1 * dist,
                c2: p3 + t2 * dist,
                b: p3,
            },
            lo: first,
            hi: last,
        });
        return;
    }

    let mut u = chord_length_params(&d[first..=last]);
    let mut bez = generate_bezier(&d[first..=last], &u, t1, t2);
    let (mut err, mut split) = max_error(&d[first..=last], &bez, &u);
    if err <= tol {
        out.push(CubicPiece {
            prim: to_prim(&bez),
            lo: first,
            hi: last,
        });
        return;
    }
    if err <= tol * 6.0 {
        for _ in 0..6 {
            u = reparameterize(&d[first..=last], &u, &bez);
            bez = generate_bezier(&d[first..=last], &u, t1, t2);
            let (e, s) = max_error(&d[first..=last], &bez, &u);
            err = e;
            split = s;
            if err <= tol {
                out.push(CubicPiece {
                    prim: to_prim(&bez),
                    lo: first,
                    hi: last,
                });
                return;
            }
        }
    }
    // Split at the point of maximum deviation (strictly inside).
    let mut s = first + split;
    s = s.clamp(first + 1, last - 1);
    if depth > 40 || n < 4 {
        // Cannot split meaningfully: accept the best effort fit.
        out.push(CubicPiece {
            prim: to_prim(&bez),
            lo: first,
            hi: last,
        });
        return;
    }
    let tc = (d[s - 1] - d[s + 1]).normalized();
    let tc = if tc == Point::ZERO {
        (d[first] - d[last]).normalized()
    } else {
        tc
    };
    fit_rec(d, first, s, t1, tc, tol, out, depth + 1);
    fit_rec(d, s, last, -tc, t2, tol, out, depth + 1);
}

fn to_prim(b: &[Point; 4]) -> Primitive {
    Primitive::Cubic {
        a: b[0],
        c1: b[1],
        c2: b[2],
        b: b[3],
    }
}

fn chord_length_params(d: &[Point]) -> Vec<f64> {
    let mut u = Vec::with_capacity(d.len());
    u.push(0.0);
    for i in 1..d.len() {
        u.push(u[i - 1] + d[i].dist(d[i - 1]));
    }
    let total = *u.last().unwrap();
    if total < 1e-12 {
        let n = d.len() as f64 - 1.0;
        return (0..d.len()).map(|i| i as f64 / n).collect();
    }
    for v in &mut u {
        *v /= total;
    }
    u
}

fn generate_bezier(d: &[Point], u: &[f64], t1: Point, t2: Point) -> [Point; 4] {
    let n = d.len();
    let p0 = d[0];
    let p3 = d[n - 1];
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let t = u[i];
        let s = 1.0 - t;
        let b0 = s * s * s;
        let b1 = 3.0 * t * s * s;
        let b2 = 3.0 * t * t * s;
        let b3 = t * t * t;
        let a1 = t1 * b1;
        let a2 = t2 * b2;
        c00 += a1.dot(a1);
        c01 += a1.dot(a2);
        c11 += a2.dot(a2);
        let tmp = d[i] - (p0 * (b0 + b1) + p3 * (b2 + b3));
        x0 += a1.dot(tmp);
        x1 += a2.dot(tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let seg = p0.dist(p3);
    let (mut al, mut ar) = if det.abs() > 1e-12 {
        ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det)
    } else {
        (0.0, 0.0)
    };
    let eps = 1e-6 * seg;
    if al < eps || ar < eps || !al.is_finite() || !ar.is_finite() {
        al = seg / 3.0;
        ar = seg / 3.0;
    }
    [p0, p0 + t1 * al, p3 + t2 * ar, p3]
}

fn max_error(d: &[Point], b: &[Point; 4], u: &[f64]) -> (f64, usize) {
    let mut max = 0.0;
    let mut at = d.len() / 2;
    for i in 1..d.len() - 1 {
        let e = cubic_point(b[0], b[1], b[2], b[3], u[i]).dist(d[i]);
        if e > max {
            max = e;
            at = i;
        }
    }
    (max, at)
}

fn reparameterize(d: &[Point], u: &[f64], b: &[Point; 4]) -> Vec<f64> {
    d.iter()
        .zip(u)
        .map(|(p, &t)| {
            let q = cubic_point(b[0], b[1], b[2], b[3], t);
            let s = 1.0 - t;
            let q1 =
                ((b[1] - b[0]) * (s * s) + (b[2] - b[1]) * (2.0 * s * t) + (b[3] - b[2]) * (t * t))
                    * 3.0;
            let q2 = ((b[2] - b[1] * 2.0 + b[0]) * s + (b[3] - b[2] * 2.0 + b[1]) * t) * 6.0;
            let num = (q - *p).dot(q1);
            let den = q1.dot(q1) + (q - *p).dot(q2);
            if den.abs() < 1e-12 {
                t
            } else {
                (t - num / den).clamp(0.0, 1.0)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_sine_wave_within_tolerance() {
        let pts: Vec<_> = (0..=100)
            .map(|i| {
                let x = i as f64 * 0.5;
                Point::new(x, 8.0 * (x / 6.0).sin())
            })
            .collect();
        let pieces = fit_cubics(&pts, 0.3, None, None);
        assert!(
            !pieces.is_empty() && pieces.len() < 10,
            "{} pieces",
            pieces.len()
        );
        // Continuity of coverage.
        assert_eq!(pieces[0].lo, 0);
        assert_eq!(pieces.last().unwrap().hi, pts.len() - 1);
        for w in pieces.windows(2) {
            assert_eq!(w[0].hi, w[1].lo);
        }
        // Every sampled point is near the produced curve.
        for p in &pts {
            let mut best = f64::MAX;
            for pc in &pieces {
                if let Primitive::Cubic { a, c1, c2, b } = pc.prim {
                    for k in 0..=200 {
                        best = best.min(cubic_point(a, c1, c2, b, k as f64 / 200.0).dist(*p));
                    }
                }
            }
            assert!(best < 0.45, "deviation {best}");
        }
    }

    #[test]
    fn two_points_make_a_degenerate_cubic() {
        let pieces = fit_cubics(
            &[Point::new(0.0, 0.0), Point::new(3.0, 0.0)],
            0.5,
            None,
            None,
        );
        assert_eq!(pieces.len(), 1);
    }
}
