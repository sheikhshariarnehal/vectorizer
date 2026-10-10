//! Curvature-discontinuity ("corner") detector on sub-pixel polylines.

use crate::geom::Point;

/// Look-around distance in px. Large enough to average out the ±0.5 px staircase jitter,
/// small enough to resolve features of a few pixels.
const WINDOW: f64 = 2.5;
const MAX_STEPS: usize = 6;
const MIN_ARM: f64 = 1.0;
/// Corners closer than this many samples suppress each other (only the sharpest survives).
const NMS_RADIUS: usize = 3;

/// Indices of corner vertices: points whose interior angle is below `max_interior_deg`.
///
/// For open polylines the end points are never reported (they are topological nodes already).
/// For closed polylines `pts` must not repeat the first point at the end.
pub fn detect_corners(pts: &[Point], closed: bool, max_interior_deg: f64) -> Vec<usize> {
    let n = pts.len();
    if n < 5 {
        return Vec::new();
    }
    let at = |i: isize| -> Option<Point> {
        if closed {
            Some(pts[i.rem_euclid(n as isize) as usize])
        } else if i < 0 || i >= n as isize {
            None
        } else {
            Some(pts[i as usize])
        }
    };
    let range: Box<dyn Iterator<Item = usize>> = if closed {
        Box::new(0..n)
    } else {
        Box::new(1..n - 1)
    };
    let mut interior_at = vec![f64::INFINITY; n];
    for i in range {
        let p = pts[i];
        let back = walk(&at, i as isize, -1, p);
        let fwd = walk(&at, i as isize, 1, p);
        let (Some(b), Some(f)) = (back, fwd) else {
            continue;
        };
        let v1 = p - b;
        let v2 = f - p;
        let (l1, l2) = (v1.len(), v2.len());
        if l1 < MIN_ARM || l2 < MIN_ARM {
            continue;
        }
        let cos = (v1.dot(v2) / (l1 * l2)).clamp(-1.0, 1.0);
        let interior = 180.0 - cos.acos().to_degrees();
        if interior < max_interior_deg {
            interior_at[i] = interior;
        }
    }
    // Non-maximum suppression: a corner is the sharpest sample within +-NMS_RADIUS samples
    // (first of a plateau). Two real corners further apart than that stay separate.
    let mut out = Vec::new();
    for i in 0..n {
        let a = interior_at[i];
        if !a.is_finite() {
            continue;
        }
        let mut is_min = true;
        for d in 1..=NMS_RADIUS as isize {
            let (before, after) = (
                at_idx(i as isize - d, n, closed),
                at_idx(i as isize + d, n, closed),
            );
            if let Some(j) = before {
                if interior_at[j] <= a {
                    is_min = false;
                    break;
                }
            }
            if let Some(j) = after {
                if interior_at[j] < a {
                    is_min = false;
                    break;
                }
            }
        }
        if is_min {
            out.push(i);
        }
    }
    out
}

fn at_idx(i: isize, n: usize, closed: bool) -> Option<usize> {
    if closed {
        Some(i.rem_euclid(n as isize) as usize)
    } else if i < 0 || i >= n as isize {
        None
    } else {
        Some(i as usize)
    }
}

/// Walk from sample `i` in direction `dir` until at least `WINDOW` px away (or the polyline
/// ends, in which case the last reachable sample is used).
fn walk(at: &impl Fn(isize) -> Option<Point>, i: isize, dir: isize, p: Point) -> Option<Point> {
    let mut last = None;
    for s in 1..=MAX_STEPS as isize {
        match at(i + dir * s) {
            Some(q) => {
                last = Some(q);
                if q.dist(p) >= WINDOW {
                    break;
                }
            }
            None => break,
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_right_angle() {
        let mut pts: Vec<Point> = (0..=10).map(|i| Point::new(i as f64, 0.0)).collect();
        pts.extend((1..=10).map(|i| Point::new(10.0, i as f64)));
        let c = detect_corners(&pts, false, 135.0);
        assert_eq!(c.len(), 1);
        assert!((c[0] as isize - 10).abs() <= 1);
    }

    #[test]
    fn smooth_curve_has_no_corners() {
        let pts: Vec<Point> = (0..=60)
            .map(|i| {
                let t = std::f64::consts::PI * i as f64 / 60.0;
                Point::new(40.0 * t.cos(), 40.0 * t.sin())
            })
            .collect();
        assert!(detect_corners(&pts, false, 135.0).is_empty());
    }

    #[test]
    fn square_loop_has_four_corners() {
        let mut pts = Vec::new();
        for i in 0..10 {
            pts.push(Point::new(i as f64, 0.0));
        }
        for i in 0..10 {
            pts.push(Point::new(10.0, i as f64));
        }
        for i in 0..10 {
            pts.push(Point::new(10.0 - i as f64, 10.0));
        }
        for i in 0..10 {
            pts.push(Point::new(0.0, 10.0 - i as f64));
        }
        assert_eq!(detect_corners(&pts, true, 135.0).len(), 4);
    }
}
