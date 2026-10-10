//! Per-chain primitive fitting: corner cuts → smoothing → corner refinement → angle
//! snapping (on the shared vertex table) → MDL primitive selection → G¹ optimisation.

use crate::geom::{fit_line_tls, intersect_lines, Point};
use crate::optimizer::{corner::detect_corners, tangency::enforce_g1};
use crate::primitives::circle_arc::fit_circle;
use crate::primitives::classifier::{fit_segment, FitConfig, Piece};
use crate::primitives::line::snap_direction;
use crate::primitives::Primitive;
use crate::topology::planar_graph::{ChainGeom, NO_NODE};
use crate::types::Params;
use rayon::prelude::*;

/// Result of fitting one chain (canonical direction).
#[derive(Clone, Debug, Default)]
pub struct ChainOut {
    pub prims: Vec<Primitive>,
    /// Sharp corner vertices detected inside the chain (for the primitive inspector).
    pub corners: Vec<Point>,
}

#[derive(Default)]
struct Plan {
    /// Vertex indices into the chain's (possibly rotated / closed-up) point list.
    cuts: Vec<usize>,
    /// Whether the vertex is a deliberate corner (true) or a smooth artificial split.
    corner_cut: Vec<bool>,
    straight: Vec<bool>,
    tangents: Vec<(Option<Point>, Option<Point>)>,
    circle: Option<Primitive>,
}

pub fn fit_all(
    chains: &mut [ChainGeom],
    nodes: &mut [Point],
    used: &[bool],
    params: &Params,
    width: usize,
    height: usize,
) -> Vec<ChainOut> {
    // A0. localise junction nodes at the true meeting point of their incident edges
    crate::junction::refine_nodes(chains, nodes, used, width, height);

    // A. per-chain preparation (parallel)
    let mut plans: Vec<Option<Plan>> = chains
        .par_iter_mut()
        .zip(used.par_iter())
        .map(|(ch, &u)| u.then(|| prepare(ch, params, width, height)))
        .collect();

    // B. snap line directions on the shared vertex table (sequential, cheap)
    if params.snap_angle > 0.0 {
        snap_vertices(chains, nodes, &plans, params, width, height);
    }
    for ch in chains.iter_mut() {
        if !ch.closed && ch.start_node != NO_NODE {
            let last = ch.points.len() - 1;
            ch.points[0] = nodes[ch.start_node as usize];
            ch.points[last] = nodes[ch.end_node as usize];
        }
    }

    // C. primitive selection per chain (parallel)
    let cfg = FitConfig {
        line_tol: params.line_tolerance,
        arc_tol: params.arc_tolerance,
        curve_tol: params.curve_tolerance,
    };
    chains
        .par_iter()
        .zip(plans.par_iter_mut())
        .map(|(ch, plan)| match plan.take() {
            Some(plan) => fit_chain(ch, plan, &cfg),
            None => ChainOut::default(),
        })
        .collect()
}

fn prepare(ch: &mut ChainGeom, p: &Params, width: usize, height: usize) -> Plan {
    let mut pts = std::mem::take(&mut ch.points);
    let n0 = pts.len();
    let mut plan = Plan::default();

    if ch.closed {
        if let Some((circle, _)) = fit_circle(&pts, p.arc_tolerance) {
            ch.points = pts;
            plan.circle = Some(circle);
            return plan;
        }
        let corners = detect_corners(&pts, true, p.corner_angle);
        if !corners.is_empty() {
            let c0 = corners[0];
            pts.rotate_left(c0);
            plan.cuts = corners.iter().map(|&c| (c + n0 - c0) % n0).collect();
            plan.cuts.push(n0);
            pts.push(pts[0]);
            plan.corner_cut = vec![true; plan.cuts.len()];
            plan.tangents = vec![(None, None); plan.cuts.len() - 1];
        } else if n0 >= 6 {
            let mid = n0 / 2;
            // Wide baseline: a +-1 sample tangent is dominated by staircase noise.
            let k = (n0 / 8).clamp(1, 6);
            let tan_at = |i: usize| (pts[(i + k) % n0] - pts[(i + n0 - k) % n0]).normalized();
            let (t0, tm) = (tan_at(0), tan_at(mid));
            plan.cuts = vec![0, mid, n0];
            plan.corner_cut = vec![false; 3];
            plan.tangents = vec![(Some(t0), Some(-tm)), (Some(tm), Some(-t0))];
            pts.push(pts[0]);
        } else {
            pts.push(pts[0]);
            plan.cuts = (0..=n0).collect();
            plan.corner_cut = vec![true; n0 + 1];
            plan.tangents = vec![(None, None); n0];
        }
    } else {
        let corners = detect_corners(&pts, false, p.corner_angle);
        plan.cuts.push(0);
        plan.cuts.extend(corners.iter().copied());
        plan.cuts.push(n0 - 1);
        plan.corner_cut = (0..plan.cuts.len())
            .map(|i| i != 0 && i != plan.cuts.len() - 1)
            .collect();
        plan.tangents = vec![(None, None); plan.cuts.len() - 1];
    }

    let nseg = plan.cuts.len() - 1;

    // Straightness of each segment, judged on the raw samples against a total-least-squares
    // line so that chamfered corner samples (up to ~0.5 px off the true edge) cannot
    // disqualify it.
    let lines: Vec<Option<(Point, Point)>> = (0..nseg)
        .map(|s| straight_line(&pts[plan.cuts[s]..=plan.cuts[s + 1]], p.line_tolerance))
        .collect();
    plan.straight = lines.iter().map(|l| l.is_some()).collect();

    // Smooth the interior of every curved segment (corners and nodes stay fixed; straight
    // segments are left untouched because smoothing would only drag them off the edge).
    for s in 0..nseg {
        let (lo, hi) = (plan.cuts[s], plan.cuts[s + 1]);
        if !plan.straight[s] && hi - lo >= 5 {
            for _ in 0..2 {
                let prev: Vec<Point> = pts[lo..=hi].to_vec();
                for i in lo + 1..hi {
                    let k = i - lo;
                    pts[i] = (prev[k - 1] + prev[k] * 2.0 + prev[k + 1]) / 4.0;
                }
            }
        }
    }

    // Replace chamfered line/line corners by the exact intersection of the fitted lines.
    let lines: Vec<Option<(Point, Point)>> = lines
        .into_iter()
        .enumerate()
        .map(|(s, l)| {
            if plan.cuts[s + 1] - plan.cuts[s] >= 4 {
                l
            } else {
                None
            }
        })
        .collect();
    let ncuts = plan.cuts.len();
    let closed = ch.closed;
    let mut moves: Vec<(usize, Point)> = Vec::new();
    for k in 0..ncuts {
        let is_open_end = !closed && (k == 0 || k == ncuts - 1);
        if is_open_end || !plan.corner_cut[k] || (closed && k == ncuts - 1) {
            continue;
        }
        let prev = if k == 0 { nseg - 1 } else { k - 1 };
        let next = if k == nseg { 0 } else { k };
        let (Some((c1, d1)), Some((c2, d2))) = (lines[prev], lines[next]) else {
            continue;
        };
        if d1.cross(d2).abs() < 0.34 {
            continue;
        }
        if let Some(x) = intersect_lines(c1, d1, c2, d2) {
            if x.dist(pts[plan.cuts[k]]) <= 1.5 {
                moves.push((plan.cuts[k], snap_to_canvas(x, width, height)));
            }
        }
    }
    for (i, x) in moves {
        pts[i] = x;
        if closed && i == 0 {
            let last = pts.len() - 1;
            pts[last] = x;
        }
    }

    ch.points = pts;
    plan
}


/// Coordinates within a hair of the canvas edge are exactly on it (the edge is a lattice line).
fn snap_to_canvas(p: Point, width: usize, height: usize) -> Point {
    let fix = |v: f64, max: f64| {
        if v.abs() < 1e-2 {
            0.0
        } else if (v - max).abs() < 1e-2 {
            max
        } else {
            v
        }
    };
    Point::new(fix(p.x, width as f64), fix(p.y, height as f64))
}

/// TLS line through the interior of a segment if every interior sample lies within `tol`.
fn straight_line(seg: &[Point], tol: f64) -> Option<(Point, Point)> {
    let n = seg.len();
    if n <= 3 {
        return Some(fit_line_tls(seg));
    }
    let inner = if n >= 8 {
        &seg[2..n - 2]
    } else if n >= 5 {
        &seg[1..n - 1]
    } else {
        seg
    };
    let (c, d) = fit_line_tls(inner);
    let worst = inner
        .iter()
        .map(|q| (*q - c).cross(d).abs())
        .fold(0.0, f64::max);
    (worst <= tol).then_some((c, d))
}

#[derive(Clone, Copy, PartialEq)]
enum VRef {
    Node(u32),
    Local(usize, usize),
}

fn snap_vertices(
    chains: &mut [ChainGeom],
    nodes: &mut [Point],
    plans: &[Option<Plan>],
    params: &Params,
    width: usize,
    height: usize,
) {
    let max_shift = params.snap_max_shift;
    let (w, h) = (width as f64, height as f64);
    let on_border = |p: Point| p.x <= 1e-9 || p.y <= 1e-9 || p.x >= w - 1e-9 || p.y >= h - 1e-9;

    for c in 0..chains.len() {
        let Some(plan) = &plans[c] else { continue };
        if plan.circle.is_some() {
            continue;
        }
        let nseg = plan.cuts.len() - 1;
        for s in 0..nseg {
            if !plan.straight[s] {
                continue;
            }
            let (i0, i1) = (plan.cuts[s], plan.cuts[s + 1]);
            let v0 = vref(&chains[c], c, i0);
            let v1 = vref(&chains[c], c, i1);
            if v0 == v1 {
                continue;
            }
            let (p0, p1) = (get(chains, nodes, v0), get(chains, nodes, v1));
            let d = p1 - p0;
            if d.len() < 3.0 {
                continue;
            }
            let Some(u) = snap_direction(d, params.snap_angle) else {
                continue;
            };
            let n = u.perp();
            let off = n.dot(d);
            if off.abs() < 1e-9 {
                continue;
            }
            let (l0, l1) = (on_border(p0), on_border(p1));
            let (m0, m1): (f64, f64) = match (l0, l1) {
                (true, true) => continue,
                (true, false) => (0.0, 1.0),
                (false, true) => (1.0, 0.0),
                (false, false) => (0.5, 0.5),
            };
            if off.abs() * m0.max(m1) > max_shift {
                continue;
            }
            set(chains, nodes, v0, p0 + n * (off * m0));
            set(chains, nodes, v1, p1 - n * (off * m1));
        }
    }
}

fn vref(ch: &ChainGeom, c: usize, idx: usize) -> VRef {
    let last = ch.points.len() - 1;
    if ch.closed {
        VRef::Local(c, if idx == last { 0 } else { idx })
    } else if idx == 0 {
        VRef::Node(ch.start_node)
    } else if idx == last {
        VRef::Node(ch.end_node)
    } else {
        VRef::Local(c, idx)
    }
}

fn get(chains: &[ChainGeom], nodes: &[Point], v: VRef) -> Point {
    match v {
        VRef::Node(id) => nodes[id as usize],
        VRef::Local(c, i) => chains[c].points[i],
    }
}

fn set(chains: &mut [ChainGeom], nodes: &mut [Point], v: VRef, p: Point) {
    match v {
        VRef::Node(id) => nodes[id as usize] = p,
        VRef::Local(c, i) => {
            let ch = &mut chains[c];
            ch.points[i] = p;
            if ch.closed && i == 0 {
                let last = ch.points.len() - 1;
                ch.points[last] = p;
            }
        }
    }
}

fn fit_chain(ch: &ChainGeom, plan: Plan, cfg: &FitConfig) -> ChainOut {
    if let Some(c) = plan.circle {
        return ChainOut {
            prims: vec![c],
            corners: Vec::new(),
        };
    }
    let pts = &ch.points;
    let nseg = plan.cuts.len() - 1;
    let mut pieces: Vec<Piece> = Vec::new();
    let mut smooth: Vec<bool> = Vec::new();

    for s in 0..nseg {
        let (lo, hi) = (plan.cuts[s], plan.cuts[s + 1]);
        let seg = &pts[lo..=hi];
        // `straight` was decided on a total-least-squares line during preparation.
        let mut seg_pieces = if plan.straight[s] {
            vec![Piece {
                prim: Primitive::Line {
                    a: seg[0],
                    b: seg[seg.len() - 1],
                },
                lo: 0,
                hi: seg.len() - 1,
            }]
        } else {
            fit_segment(seg, cfg, plan.tangents[s].0, plan.tangents[s].1)
        };
        for pc in &mut seg_pieces {
            pc.lo += lo;
            pc.hi += lo;
        }
        // joints inside a segment are smooth by construction
        if !pieces.is_empty() {
            smooth.push(!plan.corner_cut[s]);
        }
        for (k, pc) in seg_pieces.iter().enumerate() {
            if k > 0 {
                smooth.push(true);
            }
            pieces.push(*pc);
        }
    }
    if ch.closed {
        smooth.push(!plan.corner_cut[0]);
    }

    // Drop zero-length lines (keeps the path connected: both neighbours share the point).
    if pieces.len() > 1 {
        let mut i = 0;
        while i < pieces.len() && pieces.len() > 1 {
            let degenerate = matches!(pieces[i].prim, Primitive::Line { a, b } if a.dist(b) < 1e-9);
            if degenerate {
                pieces.remove(i);
                if !smooth.is_empty() {
                    smooth.remove(i.min(smooth.len() - 1));
                }
            } else {
                i += 1;
            }
        }
    }

    enforce_g1(&mut pieces, &smooth, ch.closed, pts, cfg.arc_tol);

    let corners = plan
        .cuts
        .iter()
        .zip(&plan.corner_cut)
        .enumerate()
        .filter(|(k, (_, &is_corner))| is_corner && !(ch.closed && *k == plan.cuts.len() - 1))
        .map(|(_, (&i, _))| pts[i])
        .collect();
    ChainOut {
        prims: pieces.into_iter().map(|p| p.prim).collect(),
        corners,
    }
}
