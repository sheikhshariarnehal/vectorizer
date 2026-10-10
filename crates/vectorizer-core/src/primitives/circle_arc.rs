//! Pratt algebraic circle fitting and endpoint-constrained circular arcs.

use super::Primitive;
use crate::geom::{angle_of, wrap_pi, Point};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub struct Circle {
    pub center: Point,
    pub r: f64,
}

/// Pratt's algebraic fit (constraint `B² + C² − 4AD = 1`), solved with Newton iterations on
/// the characteristic polynomial (Chernov & Lesort). Unlike Kåsa it is unbiased for short arcs.
pub fn pratt(pts: &[Point]) -> Option<Circle> {
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len() as f64;
    let (mut mx, mut my) = (0.0, 0.0);
    for p in pts {
        mx += p.x;
        my += p.y;
    }
    mx /= n;
    my /= n;
    let (mut mxx, mut myy, mut mxy, mut mxz, mut myz, mut mzz) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let x = p.x - mx;
        let y = p.y - my;
        let z = x * x + y * y;
        mxx += x * x;
        myy += y * y;
        mxy += x * y;
        mxz += x * z;
        myz += y * z;
        mzz += z * z;
    }
    mxx /= n;
    myy /= n;
    mxy /= n;
    mxz /= n;
    myz /= n;
    mzz /= n;

    let mz = mxx + myy;
    let cov_xy = mxx * myy - mxy * mxy;
    let var_z = mzz - mz * mz;
    let a2 = 4.0 * cov_xy - 3.0 * mz * mz - mzz;
    let a1 = var_z * mz + 4.0 * cov_xy * mz - mxz * mxz - myz * myz;
    let a0 = mxz * (mxz * myy - myz * mxy) + myz * (myz * mxx - mxz * mxy) - var_z * cov_xy;
    let a22 = a2 + a2;

    let mut x = 0.0f64;
    let mut y = a0;
    for _ in 0..99 {
        let dy = a1 + x * (a22 + 16.0 * x * x);
        let xnew = x - y / dy;
        if xnew == x || !xnew.is_finite() {
            break;
        }
        let ynew = a0 + xnew * (a1 + xnew * (a2 + 4.0 * xnew * xnew));
        if ynew.abs() >= y.abs() {
            break;
        }
        x = xnew;
        y = ynew;
    }
    let det = x * x - x * mz + cov_xy;
    if det.abs() < 1e-18 {
        return None;
    }
    let cx = (mxz * (myy - x) - myz * mxy) / det / 2.0;
    let cy = (myz * (mxx - x) - mxz * mxy) / det / 2.0;
    let r2 = cx * cx + cy * cy + mz + 2.0 * x;
    if !(r2.is_finite() && r2 > 0.0) {
        return None;
    }
    let c = Point::new(cx + mx, cy + my);
    if !c.is_finite() {
        return None;
    }
    Some(Circle {
        center: c,
        r: r2.sqrt(),
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ArcFit {
    pub prim: Primitive,
    pub max_err: f64,
    pub sse: f64,
    /// Total swept angle in radians (absolute).
    pub sweep_angle: f64,
}

/// Largest radius we are willing to call an arc (beyond that a line is the honest answer).
const MAX_RADIUS: f64 = 50_000.0;

/// Signed angle swept around `center` while walking `pts`; also reports whether the walk is
/// monotone (no significant backtracking).
fn sweep_of(pts: &[Point], center: Point, closed: bool) -> (f64, bool) {
    let mut total = 0.0;
    let mut abs_sum = 0.0;
    let m = pts.len();
    let last = if closed { m } else { m - 1 };
    for i in 0..last {
        let a = angle_of(pts[i] - center);
        let b = angle_of(pts[(i + 1) % m] - center);
        let d = wrap_pi(b - a);
        total += d;
        abs_sum += d.abs();
    }
    (total, total.abs() >= 0.85 * abs_sum)
}

/// Fit a circular arc that starts at `pts[0]` and ends at `pts[last]` (endpoints are fixed by
/// the topology) with radius from an unconstrained Pratt fit of all points.
pub fn fit_arc(pts: &[Point], tol: f64) -> Option<ArcFit> {
    let n = pts.len();
    if n < 4 {
        return None;
    }
    let a = pts[0];
    let b = pts[n - 1];
    let chord = b - a;
    let cl = chord.len();
    if cl < 1e-6 {
        return None;
    }
    let circ = pratt(pts)?;
    if circ.r > MAX_RADIUS {
        return None;
    }
    let mid = (a + b) * 0.5;
    let normal = chord.perp() / cl;
    let side = if (circ.center - mid).dot(normal) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let r_min = cl * 0.5 + 1e-9;
    let center_for = |r: f64| mid + normal * ((r * r - cl * cl * 0.25).max(0.0).sqrt() * side);
    let sse_for = |r: f64| {
        let c = center_for(r);
        pts[1..n - 1]
            .iter()
            .map(|p| ((*p - c).len() - r).powi(2))
            .sum::<f64>()
    };
    // The end points are fixed by the topology, so the free parameter is only the radius:
    // refine it by a bracketed 1-D search around the Pratt estimate (the sum of squared radial
    // residuals is unimodal there).
    let r0 = circ.r.max(r_min);
    let (mut lo, mut hi) = ((r0 * 0.7).max(r_min), r0 * 1.4);
    for _ in 0..48 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if sse_for(m1) < sse_for(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let r = 0.5 * (lo + hi);
    let center = center_for(r);

    let (total, monotone) = sweep_of(pts, center, false);
    if !monotone || total.abs() < 0.05 {
        return None;
    }
    let mut max_err = 0.0f64;
    let mut sse = 0.0f64;
    for p in &pts[1..n - 1] {
        let e = ((*p - center).len() - r).abs();
        max_err = max_err.max(e);
        sse += e * e;
        if max_err > tol * 4.0 {
            return None;
        }
    }
    if max_err > tol {
        return None;
    }
    let sweep = total > 0.0;
    let large = total.abs() > PI;
    Some(ArcFit {
        prim: Primitive::Arc {
            a,
            b,
            center,
            r,
            large,
            sweep,
        },
        max_err,
        sse,
        sweep_angle: total.abs(),
    })
}

/// Fit a full circle to a closed loop of points.
pub fn fit_circle(pts: &[Point], tol: f64) -> Option<(Primitive, f64)> {
    if pts.len() < 8 {
        return None;
    }
    let c = pratt(pts)?;
    if c.r > MAX_RADIUS || c.r < 0.5 {
        return None;
    }
    let mut max_err = 0.0f64;
    for p in pts {
        max_err = max_err.max(((*p - c.center).len() - c.r).abs());
        if max_err > tol {
            return None;
        }
    }
    let (total, monotone) = sweep_of(pts, c.center, true);
    if !monotone || total.abs() < TAU * 0.98 {
        return None;
    }
    let start_dir = (pts[0] - c.center).normalized();
    let start = c.center + start_dir * c.r;
    Some((
        Primitive::Circle {
            center: c.center,
            r: c.r,
            start,
            sweep: total > 0.0,
        },
        max_err,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_points(c: Point, r: f64, a0: f64, a1: f64, n: usize, noise: f64) -> Vec<Point> {
        (0..n)
            .map(|i| {
                let t = a0 + (a1 - a0) * i as f64 / (n - 1) as f64;
                let jitter = if i % 2 == 0 { noise } else { -noise };
                c + Point::new(t.cos(), t.sin()) * (r + jitter)
            })
            .collect()
    }

    #[test]
    fn pratt_recovers_full_circle() {
        let c = Point::new(30.0, -12.0);
        let pts = arc_points(c, 17.5, 0.0, TAU * 0.97, 60, 0.0);
        let f = pratt(&pts).unwrap();
        assert!(f.center.dist(c) < 1e-6 && (f.r - 17.5).abs() < 1e-6);
    }

    #[test]
    fn pratt_short_arc_is_unbiased() {
        let c = Point::new(100.0, 100.0);
        let pts = arc_points(c, 40.0, 0.3, 0.9, 40, 0.0);
        let f = pratt(&pts).unwrap();
        assert!(
            f.center.dist(c) < 1e-4 && (f.r - 40.0).abs() < 1e-4,
            "{f:?}"
        );
    }

    #[test]
    fn constrained_arc_flags() {
        let c = Point::new(0.0, 0.0);
        let pts = arc_points(c, 20.0, 0.0, 4.0, 50, 0.02); // ~229 degrees
        let f = fit_arc(&pts, 0.3).unwrap();
        match f.prim {
            Primitive::Arc {
                large, sweep, r, ..
            } => {
                assert!(large && sweep);
                assert!((r - 20.0).abs() < 0.1);
            }
            _ => panic!(),
        }
        let rev: Vec<_> = pts.iter().rev().copied().collect();
        match fit_arc(&rev, 0.3).unwrap().prim {
            Primitive::Arc { sweep, .. } => assert!(!sweep),
            _ => panic!(),
        }
    }

    #[test]
    fn straight_points_are_not_an_arc() {
        let pts: Vec<_> = (0..30)
            .map(|i| Point::new(i as f64, 0.01 * i as f64))
            .collect();
        assert!(fit_arc(&pts, 0.5).is_none());
    }

    #[test]
    fn full_circle() {
        let pts = arc_points(
            Point::new(5.0, 5.0),
            9.0,
            0.0,
            TAU * (1.0 - 1.0 / 48.0),
            48,
            0.03,
        );
        let (p, err) = fit_circle(&pts, 0.2).unwrap();
        assert!(err < 0.2);
        assert!(matches!(p, Primitive::Circle { .. }));
    }
}
