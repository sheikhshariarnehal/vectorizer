//! Multi-primitive fitting engine: lines, circular arcs / circles and cubic Béziers.

pub mod bezier;
pub mod circle_arc;
pub mod classifier;
pub mod line;

use crate::geom::{angle_of, Point};

/// A single oriented geometric primitive. Endpoints are shared with the neighbouring
/// primitive so a chain of primitives is always a connected path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Line {
        a: Point,
        b: Point,
    },
    /// Circular arc from `a` to `b`. `sweep` follows the SVG convention: `true` means the
    /// angle increases along the travel direction (clockwise on a y-down screen).
    Arc {
        a: Point,
        b: Point,
        center: Point,
        r: f64,
        large: bool,
        sweep: bool,
    },
    /// Full circle starting (and ending) at `start`.
    Circle {
        center: Point,
        r: f64,
        start: Point,
        sweep: bool,
    },
    Cubic {
        a: Point,
        c1: Point,
        c2: Point,
        b: Point,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Line,
    Arc,
    Circle,
    Cubic,
}

impl Primitive {
    pub fn kind(&self) -> PrimitiveKind {
        match self {
            Primitive::Line { .. } => PrimitiveKind::Line,
            Primitive::Arc { .. } => PrimitiveKind::Arc,
            Primitive::Circle { .. } => PrimitiveKind::Circle,
            Primitive::Cubic { .. } => PrimitiveKind::Cubic,
        }
    }

    pub fn start(&self) -> Point {
        match *self {
            Primitive::Line { a, .. } | Primitive::Arc { a, .. } | Primitive::Cubic { a, .. } => a,
            Primitive::Circle { start, .. } => start,
        }
    }

    pub fn end(&self) -> Point {
        match *self {
            Primitive::Line { b, .. } | Primitive::Arc { b, .. } | Primitive::Cubic { b, .. } => b,
            Primitive::Circle { start, .. } => start,
        }
    }

    /// The same curve traversed in the opposite direction.
    pub fn reversed(&self) -> Primitive {
        match *self {
            Primitive::Line { a, b } => Primitive::Line { a: b, b: a },
            Primitive::Arc {
                a,
                b,
                center,
                r,
                large,
                sweep,
            } => Primitive::Arc {
                a: b,
                b: a,
                center,
                r,
                large,
                sweep: !sweep,
            },
            Primitive::Circle {
                center,
                r,
                start,
                sweep,
            } => Primitive::Circle {
                center,
                r,
                start,
                sweep: !sweep,
            },
            Primitive::Cubic { a, c1, c2, b } => Primitive::Cubic {
                a: b,
                c1: c2,
                c2: c1,
                b: a,
            },
        }
    }

    /// Unit tangent (direction of travel) at the start.
    pub fn start_tangent(&self) -> Point {
        match *self {
            Primitive::Line { a, b } => (b - a).normalized(),
            Primitive::Arc {
                a, center, sweep, ..
            } => arc_tangent(a, center, sweep),
            Primitive::Circle {
                start,
                center,
                sweep,
                ..
            } => arc_tangent(start, center, sweep),
            Primitive::Cubic { a, c1, c2, b } => {
                let t = (c1 - a).normalized();
                if t == Point::ZERO {
                    let t2 = (c2 - a).normalized();
                    if t2 == Point::ZERO {
                        (b - a).normalized()
                    } else {
                        t2
                    }
                } else {
                    t
                }
            }
        }
    }

    /// Unit tangent (direction of travel) at the end.
    pub fn end_tangent(&self) -> Point {
        match *self {
            Primitive::Line { a, b } => (b - a).normalized(),
            Primitive::Arc {
                b, center, sweep, ..
            } => arc_tangent(b, center, sweep),
            Primitive::Circle {
                start,
                center,
                sweep,
                ..
            } => arc_tangent(start, center, sweep),
            Primitive::Cubic { a, c1, c2, b } => {
                let t = (b - c2).normalized();
                if t == Point::ZERO {
                    let t2 = (b - c1).normalized();
                    if t2 == Point::ZERO {
                        (b - a).normalized()
                    } else {
                        t2
                    }
                } else {
                    t
                }
            }
        }
    }

    /// Replace the start point, preserving as much shape as possible.
    pub fn with_start(&self, p: Point) -> Primitive {
        match *self {
            Primitive::Line { b, .. } => Primitive::Line { a: p, b },
            Primitive::Arc {
                b,
                center,
                r,
                large,
                sweep,
                ..
            } => rebuild_arc(p, b, center, r, large, sweep),
            Primitive::Circle { .. } => *self,
            Primitive::Cubic { a, c1, c2, b } => Primitive::Cubic {
                a: p,
                c1: c1 + (p - a),
                c2,
                b,
            },
        }
    }

    /// Replace the end point, preserving as much shape as possible.
    pub fn with_end(&self, p: Point) -> Primitive {
        match *self {
            Primitive::Line { a, .. } => Primitive::Line { a, b: p },
            Primitive::Arc {
                a,
                center,
                r,
                large,
                sweep,
                ..
            } => rebuild_arc(a, p, center, r, large, sweep),
            Primitive::Circle { .. } => *self,
            Primitive::Cubic { a, c1, c2, b } => Primitive::Cubic {
                a,
                c1,
                c2: c2 + (p - b),
                b: p,
            },
        }
    }

    /// Rough length used for statistics / heuristics.
    pub fn approx_length(&self) -> f64 {
        match *self {
            Primitive::Line { a, b } => a.dist(b),
            Primitive::Arc { a, b, r, large, .. } => {
                let chord = a.dist(b);
                let half = (chord / (2.0 * r)).clamp(-1.0, 1.0).asin();
                let ang = if large {
                    2.0 * std::f64::consts::PI - 2.0 * half
                } else {
                    2.0 * half
                };
                r * ang
            }
            Primitive::Circle { r, .. } => 2.0 * std::f64::consts::PI * r,
            Primitive::Cubic { a, c1, c2, b } => {
                0.5 * (a.dist(b) + a.dist(c1) + c1.dist(c2) + c2.dist(b))
            }
        }
    }

    /// Polyline approximation; `tol` is the maximum sagitta in px.
    pub fn flatten(&self, tol: f64, out: &mut Vec<Point>) {
        match *self {
            Primitive::Line { b, .. } => out.push(b),
            Primitive::Arc {
                a,
                b,
                center,
                r,
                large,
                sweep,
            } => {
                let (start, sw) = arc_angles(a, b, center, large, sweep);
                flatten_arc(center, r, start, sw, tol, out);
            }
            Primitive::Circle {
                center,
                r,
                start,
                sweep,
            } => {
                let a0 = angle_of(start - center);
                let sw = if sweep {
                    std::f64::consts::TAU
                } else {
                    -std::f64::consts::TAU
                };
                flatten_arc(center, r, a0, sw, tol, out);
            }
            Primitive::Cubic { a, c1, c2, b } => {
                let n = (((a.dist(c1) + c1.dist(c2) + c2.dist(b)) / tol.max(0.05)).sqrt() as usize)
                    .clamp(4, 64);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    out.push(cubic_point(a, c1, c2, b, t));
                }
            }
        }
    }
}

fn flatten_arc(center: Point, r: f64, start: f64, sweep: f64, tol: f64, out: &mut Vec<Point>) {
    let step = 2.0 * (1.0 - (tol / r.max(tol)).min(0.99)).acos();
    let n = ((sweep.abs() / step.max(0.02)).ceil() as usize).clamp(2, 720);
    for i in 1..=n {
        let a = start + sweep * i as f64 / n as f64;
        out.push(center + Point::new(a.cos(), a.sin()) * r);
    }
}

#[inline]
fn arc_tangent(p: Point, center: Point, sweep: bool) -> Point {
    let t = (p - center).normalized().perp();
    if sweep {
        t
    } else {
        -t
    }
}

/// Start angle and signed sweep of an arc.
pub fn arc_angles(a: Point, b: Point, center: Point, large: bool, sweep: bool) -> (f64, f64) {
    use std::f64::consts::TAU;
    let a0 = angle_of(a - center);
    let a1 = angle_of(b - center);
    let mut d = a1 - a0;
    if sweep {
        while d < 0.0 {
            d += TAU;
        }
        while d >= TAU {
            d -= TAU;
        }
        if large != (d > std::f64::consts::PI) {
            d = if large {
                d.max(std::f64::consts::PI)
            } else {
                d.min(std::f64::consts::PI)
            };
        }
    } else {
        while d > 0.0 {
            d -= TAU;
        }
        while d <= -TAU {
            d += TAU;
        }
        if large != (d < -std::f64::consts::PI) {
            d = if large {
                d.min(-std::f64::consts::PI)
            } else {
                d.max(-std::f64::consts::PI)
            };
        }
    }
    (a0, d)
}

/// Rebuild an arc after one endpoint moved: keeps the radius, recomputes the center on
/// the same side of the new chord.
fn rebuild_arc(
    a: Point,
    b: Point,
    old_center: Point,
    r: f64,
    large: bool,
    sweep: bool,
) -> Primitive {
    let chord = b - a;
    let cl = chord.len();
    if cl < 1e-9 {
        return Primitive::Line { a, b };
    }
    let r = r.max(cl * 0.5 + 1e-9);
    let mid = (a + b) * 0.5;
    let n = chord.perp() / cl;
    let h = (r * r - cl * cl * 0.25).max(0.0).sqrt();
    // Pick the center side that matches the previous center.
    let s = if (old_center - mid).dot(n) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let center = mid + n * (h * s);
    Primitive::Arc {
        a,
        b,
        center,
        r,
        large,
        sweep,
    }
}

#[inline]
pub fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}
