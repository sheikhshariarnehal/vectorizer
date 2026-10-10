//! G¹ tangent-continuity solver for the joints between consecutive primitives.
//!
//! Where two primitives meet with their tangents less than 10° apart the joint is meant to
//! be smooth; the more flexible primitive is adjusted so both share one tangent direction:
//! `t = normalize(t_out + t_in)` for Bézier–Bézier, the rigid neighbour's tangent otherwise.

use crate::geom::{angle_of, Point};
use crate::primitives::classifier::Piece;
use crate::primitives::Primitive;
use std::f64::consts::{PI, TAU};

const G1_ANGLE_DEG: f64 = 10.0;

/// `smooth[i]` says whether the joint between `pieces[i]` and `pieces[i + 1]` (wrapping for
/// closed chains) is a smooth joint rather than a deliberate corner.
pub fn enforce_g1(
    pieces: &mut [Piece],
    smooth: &[bool],
    closed: bool,
    pts: &[Point],
    arc_tol: f64,
) {
    let n = pieces.len();
    if n < 2 {
        if n == 1 && closed && smooth.first().copied().unwrap_or(false) {
            // single primitive closing on itself: nothing to align
        }
        return;
    }
    let joints = if closed { n } else { n - 1 };
    for i in 0..joints {
        if !smooth.get(i).copied().unwrap_or(false) {
            continue;
        }
        let j = (i + 1) % n;
        if i == j {
            continue;
        }
        let (p, q) = (pieces[i].prim, pieces[j].prim);
        let (tp, tq) = (p.end_tangent(), q.start_tangent());
        if tp == Point::ZERO || tq == Point::ZERO {
            continue;
        }
        let ang = tp.dot(tq).clamp(-1.0, 1.0).acos().to_degrees();
        if ang > G1_ANGLE_DEG {
            continue;
        }
        let joint = p.end();
        let (np, nq) = match (p, q) {
            (
                Primitive::Cubic { a, c1, c2, b },
                Primitive::Cubic {
                    a: qa,
                    c1: qc1,
                    c2: qc2,
                    b: qb,
                },
            ) => {
                let t = (tp + tq).normalized();
                if t == Point::ZERO {
                    continue;
                }
                (
                    Primitive::Cubic {
                        a,
                        c1,
                        c2: joint - t * (c2 - b).len(),
                        b,
                    },
                    Primitive::Cubic {
                        a: qa,
                        c1: qa + t * (qc1 - qa).len(),
                        c2: qc2,
                        b: qb,
                    },
                )
            }
            (Primitive::Cubic { a, c1, c2, b }, Primitive::Line { .. } | Primitive::Arc { .. }) => {
                (
                    Primitive::Cubic {
                        a,
                        c1,
                        c2: joint - tq * (c2 - b).len(),
                        b,
                    },
                    q,
                )
            }
            (Primitive::Line { .. } | Primitive::Arc { .. }, Primitive::Cubic { a, c1, c2, b }) => {
                (
                    p,
                    Primitive::Cubic {
                        a,
                        c1: joint + tp * (c1 - a).len(),
                        c2,
                        b,
                    },
                )
            }
            (Primitive::Line { .. }, Primitive::Arc { .. }) => {
                match tangent_arc(&pieces[j], tp, pts, arc_tol) {
                    Some(arc) => (p, arc),
                    None => continue,
                }
            }
            (Primitive::Arc { .. }, Primitive::Line { .. }) => {
                let rev = Piece {
                    prim: p.reversed(),
                    ..pieces[i]
                };
                match tangent_arc(&rev, -tq, pts, arc_tol) {
                    Some(arc) => (arc.reversed(), q),
                    None => continue,
                }
            }
            _ => continue,
        };
        pieces[i].prim = np;
        pieces[j].prim = nq;
    }
}

/// Recompute `arc` (starting at `a`) as the circle through its endpoints whose tangent at `a`
/// is `t`. Rejected if it strays more than 1.5·tol from the fitted points.
fn tangent_arc(arc: &Piece, t: Point, pts: &[Point], tol: f64) -> Option<Primitive> {
    let Primitive::Arc { a, b, sweep, .. } = arc.prim else {
        return None;
    };
    let r_dir = if sweep {
        Point::new(t.y, -t.x)
    } else {
        Point::new(-t.y, t.x)
    };
    let ba = b - a;
    let denom = 2.0 * r_dir.dot(ba);
    if denom.abs() < 1e-9 {
        return None;
    }
    let r = -ba.len2() / denom;
    if !(r > 0.0 && r < 50_000.0) {
        return None;
    }
    let center = a - r_dir * r;
    let a0 = angle_of(a - center);
    let a1 = angle_of(b - center);
    let mut d = a1 - a0;
    if sweep {
        d = d.rem_euclid(TAU);
    } else {
        d = -((-d).rem_euclid(TAU));
    }
    let large = d.abs() > PI;
    let hi = arc.hi.min(pts.len().saturating_sub(1));
    for p in &pts[arc.lo.min(hi)..=hi] {
        if ((*p - center).len() - r).abs() > tol * 1.5 {
            return None;
        }
    }
    Some(Primitive::Arc {
        a,
        b,
        center,
        r,
        large,
        sweep,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_two_cubics() {
        let a = Primitive::Cubic {
            a: Point::new(0.0, 0.0),
            c1: Point::new(5.0, 0.0),
            c2: Point::new(10.0, 0.8),
            b: Point::new(15.0, 1.0),
        };
        let b = Primitive::Cubic {
            a: Point::new(15.0, 1.0),
            c1: Point::new(20.0, 0.7),
            c2: Point::new(25.0, 5.0),
            b: Point::new(30.0, 5.0),
        };
        let mut pieces = [
            Piece {
                prim: a,
                lo: 0,
                hi: 5,
            },
            Piece {
                prim: b,
                lo: 5,
                hi: 10,
            },
        ];
        enforce_g1(&mut pieces, &[true], false, &[], 1.0);
        let (t1, t2) = (pieces[0].prim.end_tangent(), pieces[1].prim.start_tangent());
        assert!((t1 - t2).len() < 1e-9);
    }

    #[test]
    fn corners_are_untouched() {
        let l1 = Primitive::Line {
            a: Point::new(0.0, 0.0),
            b: Point::new(5.0, 0.0),
        };
        let l2 = Primitive::Cubic {
            a: Point::new(5.0, 0.0),
            c1: Point::new(8.0, 0.4),
            c2: Point::new(9.0, 2.0),
            b: Point::new(10.0, 4.0),
        };
        let mut pieces = [
            Piece {
                prim: l1,
                lo: 0,
                hi: 5,
            },
            Piece {
                prim: l2,
                lo: 5,
                hi: 10,
            },
        ];
        let before = pieces[1].prim;
        enforce_g1(&mut pieces, &[false], false, &[], 1.0);
        assert_eq!(pieces[1].prim, before);
    }
}
