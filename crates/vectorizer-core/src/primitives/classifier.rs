//! Minimum-description-length model selection between lines, arcs and Béziers.
//!
//! `Cost(M) = Σ dist(pᵢ, M)² + λ · Complexity(M)` with complexity 2 / 3 / 6 parameters for
//! line / arc / Bézier, so the simplest primitive that explains the data always wins.

use super::bezier::fit_cubics;
use super::circle_arc::fit_arc;
use super::line::chord_fit;
use super::{cubic_point, Primitive};
use crate::geom::Point;

/// Points ignored at each end when judging straightness (corner staircase samples).
pub const LINE_SKIP: usize = 2;

#[derive(Clone, Copy, Debug)]
pub struct FitConfig {
    pub line_tol: f64,
    pub arc_tol: f64,
    pub curve_tol: f64,
}

/// A primitive together with the point range `lo..=hi` (indices into the fitted slice).
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub prim: Primitive,
    pub lo: usize,
    pub hi: usize,
}

const LINE_PARAMS: f64 = 2.0;
const ARC_PARAMS: f64 = 3.0;
const CUBIC_PARAMS: f64 = 6.0;
const HYBRID_MAX_POINTS: usize = 4000;
/// Minimum samples of the straight run that anchors a line+arc hybrid.
const MIN_ANCHOR_LINE: usize = 14;
const MIN_HYBRID_ARC: usize = 5;
/// Shortest sample run in which a circular arc is trusted during greedy decomposition.
const MIN_ARM_SAMPLES: usize = 12;

/// Fit one corner-to-corner segment. Endpoints of the result are exactly `pts[0]` and
/// `pts[last]`.
pub fn fit_segment(
    pts: &[Point],
    cfg: &FitConfig,
    t_start: Option<Point>,
    t_end: Option<Point>,
) -> Vec<Piece> {
    let n = pts.len();
    let a = pts[0];
    let b = pts[n - 1];
    let line_piece = |sse_hint: f64| {
        let _ = sse_hint;
        vec![Piece {
            prim: Primitive::Line { a, b },
            lo: 0,
            hi: n - 1,
        }]
    };
    if n <= 2 {
        return line_piece(0.0);
    }
    let closed_loop = a.dist(b) < 1e-9;
    let lambda = cfg.line_tol * cfg.line_tol;

    if !closed_loop {
        let line = chord_fit(pts, LINE_SKIP);
        let line_cost = (line.max_err <= cfg.line_tol).then(|| line.sse + lambda * LINE_PARAMS);
        let arc = if n >= 5 {
            fit_arc(pts, cfg.arc_tol)
        } else {
            None
        };
        let arc_cost = arc.map(|f| f.sse + lambda * ARC_PARAMS);
        match (line_cost, arc_cost) {
            (Some(lc), Some(ac)) => {
                return if ac < lc {
                    vec![Piece {
                        prim: arc.unwrap().prim,
                        lo: 0,
                        hi: n - 1,
                    }]
                } else {
                    line_piece(lc)
                }
            }
            (Some(lc), None) => return line_piece(lc),
            (None, Some(_)) => {
                return vec![Piece {
                    prim: arc.unwrap().prim,
                    lo: 0,
                    hi: n - 1,
                }]
            }
            (None, None) => {}
        }
    }

    let cubics: Vec<Piece> = fit_cubics(pts, cfg.curve_tol, t_start, t_end)
        .into_iter()
        .map(|c| Piece {
            prim: c.prim,
            lo: c.lo,
            hi: c.hi,
        })
        .collect();

    if !closed_loop && (12..=HYBRID_MAX_POINTS).contains(&n) {
        if let Some((hybrid, hybrid_cost)) = greedy_line_arc(pts, cfg, lambda) {
            // Every hybrid piece already satisfies a tightened tolerance, so residuals below
            // the sampling-noise floor carry no information: whenever the hybrid needs no more
            // primitives than the Bézier chain it is the simpler description and wins. Only a
            // hybrid that is *longer* has to beat the Béziers on full MDL cost.
            let cubic_cost = cubic_sse(pts, &cubics) + lambda * CUBIC_PARAMS * cubics.len() as f64;
            if hybrid.len() <= cubics.len() || hybrid_cost <= cubic_cost {
                return hybrid;
            }
        }
    }
    cubics
}

/// Longest `j` (≥ `i + min_len`) such that `ok(j)` holds, assuming monotonicity; `i` if none.
fn longest(i: usize, last: usize, min_len: usize, ok: impl Fn(usize) -> bool) -> usize {
    if i + min_len > last || !ok(i + min_len) {
        return i;
    }
    let mut good = i + min_len;
    let mut step = 4;
    let mut bad = last + 1;
    while good + step <= last {
        if ok(good + step) {
            good += step;
            step *= 2;
        } else {
            bad = good + step;
            break;
        }
    }
    if bad > last && good < last && ok(last) {
        return last;
    }
    let mut lo = good;
    let mut hi = bad.min(last + 1);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Greedy decomposition into lines and arcs (e.g. the straight sides and rounded corners of
/// a rounded rectangle, which have no curvature *corner* between them).
fn greedy_line_arc(pts: &[Point], cfg: &FitConfig, lambda: f64) -> Option<(Vec<Piece>, f64)> {
    let n = pts.len();
    let (lt, at) = (cfg.line_tol * 0.6, cfg.arc_tol * 0.6);
    let mut pieces: Vec<Piece> = Vec::new();
    let mut cost = 0.0;
    let (mut lines, mut arcs) = (0, 0);
    let mut i = 0;
    while i < n - 1 {
        let jl = longest(i, n - 1, 3, |j| {
            chord_fit(&pts[i..=j], LINE_SKIP).max_err <= lt
        });
        // Arcs need enough samples to be distinguishable from a noisy straight run.
        let ja = longest(i, n - 1, MIN_ARM_SAMPLES, |j| {
            fit_arc(&pts[i..=j], at).is_some()
        });
        // A line wins unless an arc reaches clearly further (i.e. the data really bends).
        let (prim, j, c) = if jl > i && (ja == i || (jl - i >= 3 && jl + 4 >= ja)) {
            let f = chord_fit(&pts[i..=jl], LINE_SKIP);
            lines += 1;
            (
                Primitive::Line {
                    a: pts[i],
                    b: pts[jl],
                },
                jl,
                f.sse + lambda * LINE_PARAMS,
            )
        } else if ja > i {
            let f = fit_arc(&pts[i..=ja], at)?;
            arcs += 1;
            (f.prim, ja, f.sse + lambda * ARC_PARAMS)
        } else {
            return None;
        };
        if j - i < 3 && j != n - 1 {
            return None;
        }
        pieces.push(Piece { prim, lo: i, hi: j });
        cost += c;
        i = j;
        if pieces.len() > 10 {
            return None;
        }
    }
    // Adjacent arcs are one arc if the union still fits; if it does not, the stretch is a
    // freeform curve (arc–arc joints cannot be made G¹ by moving a handle), so reject.
    let mut k = 0;
    while k + 1 < pieces.len() {
        if let (Primitive::Arc { .. }, Primitive::Arc { .. }) = (pieces[k].prim, pieces[k + 1].prim)
        {
            let (lo, hi) = (pieces[k].lo, pieces[k + 1].hi);
            let merged = fit_arc(&pts[lo..=hi], cfg.arc_tol)?;
            pieces[k] = Piece {
                prim: merged.prim,
                lo,
                hi,
            };
            pieces.remove(k + 1);
            arcs -= 1;
        } else {
            k += 1;
        }
    }
    // A hybrid is only credible when a genuinely long straight run anchors it (the sides of a
    // rounded rectangle). Short "lines" on a freeform curve are just a coarse approximation of
    // it and must stay Béziers.
    let has_long_line = pieces
        .iter()
        .any(|p| matches!(p.prim, Primitive::Line { .. }) && p.hi - p.lo >= MIN_ANCHOR_LINE);
    let arcs_ok = pieces
        .iter()
        .all(|p| !matches!(p.prim, Primitive::Arc { .. }) || p.hi - p.lo >= MIN_HYBRID_ARC);
    (pieces.len() >= 2 && lines >= 1 && arcs >= 1 && has_long_line && arcs_ok)
        .then_some((pieces, cost))
}

/// Σ squared distance of each point to its own cubic (sampled).
fn cubic_sse(pts: &[Point], pieces: &[Piece]) -> f64 {
    let mut sse = 0.0;
    for pc in pieces {
        if let Primitive::Cubic { a, c1, c2, b } = pc.prim {
            let poly: Vec<Point> = (0..=24)
                .map(|k| cubic_point(a, c1, c2, b, k as f64 / 24.0))
                .collect();
            for p in &pts[pc.lo..=pc.hi] {
                let mut best = f64::MAX;
                for w in poly.windows(2) {
                    let d = w[1] - w[0];
                    let l2 = d.len2();
                    let t = if l2 < 1e-18 {
                        0.0
                    } else {
                        ((*p - w[0]).dot(d) / l2).clamp(0.0, 1.0)
                    };
                    best = best.min(p.dist(w[0] + d * t));
                }
                sse += best * best;
            }
        }
    }
    sse
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn cfg() -> FitConfig {
        FitConfig {
            line_tol: 0.8,
            arc_tol: 0.8,
            curve_tol: 0.8,
        }
    }

    #[test]
    fn straight_segment_is_a_line() {
        let pts: Vec<_> = (0..=30)
            .map(|i| Point::new(i as f64, 0.1 * ((i % 3) as f64 - 1.0)))
            .collect();
        let mut p = pts;
        p[0].y = 0.0;
        p[30].y = 0.0;
        let out = fit_segment(&p, &cfg(), None, None);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].prim, Primitive::Line { .. }));
    }

    #[test]
    fn quarter_circle_is_an_arc() {
        let pts: Vec<_> = (0..=40)
            .map(|i| {
                let t = FRAC_PI_2 * i as f64 / 40.0;
                Point::new(30.0 * t.cos(), 30.0 * t.sin())
            })
            .collect();
        let out = fit_segment(&pts, &cfg(), None, None);
        assert_eq!(out.len(), 1);
        assert!(
            matches!(out[0].prim, Primitive::Arc { .. }),
            "{:?}",
            out[0].prim
        );
    }

    #[test]
    fn rounded_corner_profile_is_line_plus_arc() {
        // straight run, then a quarter circle tangent to it
        let mut pts: Vec<Point> = (0..=30).map(|i| Point::new(i as f64, 0.0)).collect();
        let r = 20.0;
        for i in 1..=40 {
            let t = FRAC_PI_2 * i as f64 / 40.0;
            pts.push(Point::new(30.0 + r * t.sin(), r - r * t.cos()));
        }
        let out = fit_segment(&pts, &cfg(), None, None);
        let kinds: Vec<_> = out.iter().map(|p| p.prim.kind()).collect();
        assert!(
            kinds.contains(&crate::primitives::PrimitiveKind::Line),
            "{kinds:?}"
        );
        assert!(
            kinds.contains(&crate::primitives::PrimitiveKind::Arc),
            "{kinds:?}"
        );
    }

    /// Two sides and two large tangent corners, sampled with staircase-like jitter.
    #[test]
    fn large_rounded_profile_is_lines_and_arcs() {
        let r = 72.0;
        let mut pts: Vec<Point> = Vec::new();
        let jitter = |i: usize| {
            if i % 3 == 0 {
                0.12
            } else if i % 3 == 1 {
                -0.1
            } else {
                0.0
            }
        };
        let mut k = 0;
        for i in 0..=200 {
            pts.push(Point::new(i as f64, jitter(k)));
            k += 1;
        }
        let n_arc = 113;
        for i in 1..=n_arc {
            let t = FRAC_PI_2 * i as f64 / n_arc as f64;
            let rr = r + jitter(k);
            k += 1;
            pts.push(Point::new(200.0 + rr * t.sin(), r - rr * t.cos()));
        }
        for i in 1..=150 {
            pts.push(Point::new(200.0 + r + jitter(k), r + i as f64));
            k += 1;
        }
        for i in 1..=n_arc {
            let t = FRAC_PI_2 * i as f64 / n_arc as f64;
            let rr = r + jitter(k);
            k += 1;
            pts.push(Point::new(200.0 + rr * t.cos(), r + 150.0 + rr * t.sin()));
        }
        let cfg = FitConfig {
            line_tol: 0.8,
            arc_tol: 0.7,
            curve_tol: 0.8,
        };
        let out = fit_segment(&pts, &cfg, None, None);
        let kinds: Vec<_> = out.iter().map(|p| p.prim.kind()).collect();
        assert!(
            kinds
                .iter()
                .all(|k| *k != crate::primitives::PrimitiveKind::Cubic),
            "{kinds:?}"
        );
        assert!(out.len() <= 6, "{kinds:?}");
    }

    #[test]
    fn s_curve_is_bezier() {
        let pts: Vec<_> = (0..=60)
            .map(|i| {
                let x = i as f64;
                Point::new(x, 10.0 * (x * PI / 30.0).sin())
            })
            .collect();
        let out = fit_segment(&pts, &cfg(), None, None);
        assert!(out
            .iter()
            .all(|p| matches!(p.prim, Primitive::Cubic { .. })));
    }
}
