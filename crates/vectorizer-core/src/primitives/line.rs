//! Straight-line fitting with orthogonal / diagonal angle snapping.

use crate::geom::Point;
use std::f64::consts::FRAC_PI_4;

#[derive(Clone, Copy, Debug)]
pub struct LineFit {
    /// Maximum orthogonal residual of the fitted points.
    pub max_err: f64,
    /// Sum of squared residuals.
    pub sse: f64,
}

/// Residuals of `pts` against the segment `pts[0] → pts[last]` (endpoints are fixed by the
/// topology). Besides the two end points themselves, the `skip` samples next to each end
/// are ignored: they sit in the corner neighbourhood where a staircase sample can lie a
/// fraction of a pixel off the true edge.
pub fn chord_fit(pts: &[Point], skip: usize) -> LineFit {
    let n = pts.len();
    if n < 3 {
        return LineFit {
            max_err: 0.0,
            sse: 0.0,
        };
    }
    let a = pts[0];
    let b = pts[n - 1];
    let d = b - a;
    let l2 = d.len2();
    let skip = skip.min((n - 2) / 2);
    let mut max_err = 0.0f64;
    let mut sse = 0.0f64;
    for p in &pts[1 + skip..n - 1 - skip] {
        let e = if l2 < 1e-12 {
            p.dist(a)
        } else {
            let t = ((*p - a).dot(d) / l2).clamp(0.0, 1.0);
            p.dist(a + d * t)
        };
        max_err = max_err.max(e);
        sse += e * e;
    }
    LineFit { max_err, sse }
}

/// If direction `dir` is within `tol_deg` of a multiple of 45°, return the exact unit
/// direction (orientation preserved).
pub fn snap_direction(dir: Point, tol_deg: f64) -> Option<Point> {
    if tol_deg <= 0.0 || dir.len2() < 1e-18 {
        return None;
    }
    let theta = dir.y.atan2(dir.x);
    let m = (theta / FRAC_PI_4).round();
    let target = m * FRAC_PI_4;
    if (theta - target).abs().to_degrees() <= tol_deg {
        Some(Point::new(target.cos(), target.sin()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_noisy_line() {
        let pts: Vec<_> = (0..20)
            .map(|i| Point::new(i as f64, if i % 2 == 0 { 0.05 } else { -0.05 }))
            .collect();
        let mut p = pts.clone();
        p[0] = Point::new(0.0, 0.0);
        p[19] = Point::new(19.0, 0.0);
        assert!(chord_fit(&p, 1).max_err < 0.06);
    }

    #[test]
    fn bowed_line_is_rejected_by_tolerance() {
        let pts: Vec<_> = (0..=20)
            .map(|i| {
                Point::new(
                    i as f64,
                    3.0 * (std::f64::consts::PI * i as f64 / 20.0).sin(),
                )
            })
            .collect();
        assert!(chord_fit(&pts, 1).max_err > 2.5);
    }

    #[test]
    fn snapping() {
        let s = snap_direction(Point::new(100.0, 1.0), 2.0).unwrap();
        assert!((s.x - 1.0).abs() < 1e-12 && s.y.abs() < 1e-12);
        let s = snap_direction(Point::new(-1.0, -1.02), 2.0).unwrap();
        assert!(s.x < 0.0 && s.y < 0.0 && (s.x - s.y).abs() < 1e-12);
        assert!(snap_direction(Point::new(10.0, 3.0), 2.0).is_none());
    }
}
