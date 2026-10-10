//! Small 2-D geometry toolkit shared by every stage of the engine.

use std::f64::consts::{PI, TAU};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    #[inline]
    pub fn dot(self, o: Point) -> f64 {
        self.x * o.x + self.y * o.y
    }
    #[inline]
    pub fn cross(self, o: Point) -> f64 {
        self.x * o.y - self.y * o.x
    }
    #[inline]
    pub fn len2(self) -> f64 {
        self.dot(self)
    }
    #[inline]
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }
    #[inline]
    pub fn dist(self, o: Point) -> f64 {
        (self - o).len()
    }
    /// Unit vector, or zero when the input is (numerically) zero.
    #[inline]
    pub fn normalized(self) -> Point {
        let l = self.len();
        if l < 1e-12 {
            Point::ZERO
        } else {
            self / l
        }
    }
    /// Rotate by +90 degrees (in the mathematical sense of the raw coordinates).
    #[inline]
    pub fn perp(self) -> Point {
        Point::new(-self.y, self.x)
    }
    #[inline]
    pub fn lerp(self, o: Point, t: f64) -> Point {
        self + (o - self) * t
    }
    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Point {
    type Output = Point;
    #[inline]
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Point {
    type Output = Point;
    #[inline]
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Point {
    type Output = Point;
    #[inline]
    fn mul(self, s: f64) -> Point {
        Point::new(self.x * s, self.y * s)
    }
}
impl Div<f64> for Point {
    type Output = Point;
    #[inline]
    fn div(self, s: f64) -> Point {
        Point::new(self.x / s, self.y / s)
    }
}
impl Neg for Point {
    type Output = Point;
    #[inline]
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

/// Wrap an angle to `(-PI, PI]`.
#[inline]
pub fn wrap_pi(mut a: f64) -> f64 {
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

/// Angle of the vector `p` (atan2 in the raw coordinate frame).
#[inline]
pub fn angle_of(p: Point) -> f64 {
    p.y.atan2(p.x)
}

/// Perpendicular distance from `p` to the *infinite* line through `a` and `b`.
#[inline]
pub fn dist_to_line(p: Point, a: Point, b: Point) -> f64 {
    let d = b - a;
    let l = d.len();
    if l < 1e-12 {
        p.dist(a)
    } else {
        ((p - a).cross(d) / l).abs()
    }
}

/// Total-least-squares line through `pts`: returns `(centroid, unit direction)`.
pub fn fit_line_tls(pts: &[Point]) -> (Point, Point) {
    let n = pts.len().max(1) as f64;
    let mut c = Point::ZERO;
    for p in pts {
        c = c + *p;
    }
    c = c / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in pts {
        let d = *p - c;
        sxx += d.x * d.x;
        syy += d.y * d.y;
        sxy += d.x * d.y;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    (c, Point::new(theta.cos(), theta.sin()))
}

/// Intersection of two infinite lines given as (point, direction). `None` if near-parallel.
pub fn intersect_lines(p1: Point, d1: Point, p2: Point, d2: Point) -> Option<Point> {
    let den = d1.cross(d2);
    if den.abs() < 1e-6 {
        return None;
    }
    let t = (p2 - p1).cross(d2) / den;
    Some(p1 + d1 * t)
}

/// Approximate a circular arc (center, radius, start angle, signed sweep) with cubic Béziers.
/// Returns `[p0, c1, c2, p3]` quadruples, each covering at most 90 degrees.
pub fn arc_to_cubics(center: Point, r: f64, start: f64, sweep: f64) -> Vec<[Point; 4]> {
    let n = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a0 = start + step * i as f64;
        let a1 = a0 + step;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let p0 = center + Point::new(c0, s0) * r;
        let p3 = center + Point::new(c1, s1) * r;
        let q1 = p0 + Point::new(-s0, c0) * (r * k);
        let q2 = p3 - Point::new(-s1, c1) * (r * k);
        out.push([p0, q1, q2, p3]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_line_is_exact_on_collinear_points() {
        let pts: Vec<_> = (0..10)
            .map(|i| Point::new(i as f64, 2.0 * i as f64 + 1.0))
            .collect();
        let (c, d) = fit_line_tls(&pts);
        for p in &pts {
            assert!(((*p - c).cross(d)).abs() < 1e-9);
        }
    }

    #[test]
    fn line_intersection() {
        let p = intersect_lines(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(3.0, -5.0),
            Point::new(0.0, 1.0),
        )
        .unwrap();
        assert!((p.x - 3.0).abs() < 1e-12 && p.y.abs() < 1e-12);
    }
}
