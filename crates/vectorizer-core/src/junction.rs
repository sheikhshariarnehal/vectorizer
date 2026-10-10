//! Junction localisation.
//!
//! Multi-colour junction nodes start on pixel-corner lattice positions, which can lie
//! 0.5–1 px from where the regions truly meet. Every incident edge is first modelled *freely*
//! (a circle or a line fitted to its first stretch, independent of the node), then the node is
//! moved to the point that is closest, in the least-squares sense, to all of those curves
//! (Gauss–Newton). Fitted arcs and lines are later forced through the node, so a precise node
//! is what keeps them faithful to the data.

use crate::geom::{fit_line_tls, Point};
use crate::primitives::circle_arc::pratt;
use crate::topology::planar_graph::{ChainGeom, NO_NODE};

/// Length (px) of the stretch next to a node that is used to model an edge.
const MODEL_LENGTH: f64 = 40.0;
/// Samples closer than this to the node are ignored (staircase / chamfer distortion).
const SKIP_NEAR: f64 = 6.0;
const MIN_SAMPLES: usize = 8;
const MAX_MODEL_ERR: f64 = 0.45;
/// Largest correction when the incident edge models do not agree with the new position.
const MAX_SHIFT: f64 = 1.0;
/// Largest correction when every incident edge passes within `AGREE_TOL` of the new position
/// (shallow junctions of blurred edges can sit several pixels from the lattice corner).
const MAX_SHIFT_TRUSTED: f64 = 8.0;
const AGREE_TOL: f64 = 0.35;
const PRIOR: f64 = 0.02;

#[derive(Clone, Copy)]
enum Model {
    Line { c: Point, n: Point },
    Circle { c: Point, r: f64 },
}

impl Model {
    /// Signed distance of `p` to the curve and its gradient.
    fn eval(&self, p: Point) -> (f64, Point) {
        match *self {
            Model::Line { c, n } => (n.dot(p - c), n),
            Model::Circle { c, r } => {
                let v = p - c;
                let l = v.len().max(1e-9);
                (l - r, v / l)
            }
        }
    }
}

/// Model the stretch of `pts` (ordered from the node outwards), shrinking the window until a
/// circle or line explains it well.
fn model_edge(pts: &[Point]) -> Option<Model> {
    let origin = pts[0];
    let mut length = MODEL_LENGTH;
    while length >= SKIP_NEAR + 6.0 {
        let sel: Vec<Point> = pts
            .iter()
            .copied()
            .skip(1)
            .take_while(|p| p.dist(origin) <= length)
            .filter(|p| p.dist(origin) >= SKIP_NEAR)
            .collect();
        if sel.len() >= MIN_SAMPLES {
            // A curve is only preferred when it is clearly curved: otherwise the radius of a
            // near-straight stretch is ill-defined and would bias the node.
            let (c, d) = fit_line_tls(&sel);
            let n = d.perp();
            let line_err = sel.iter().map(|p| n.dot(*p - c).abs()).fold(0.0, f64::max);
            if line_err <= 0.12 {
                return Some(Model::Line { c, n });
            }
            if let Some(circ) = pratt(&sel) {
                let err = sel
                    .iter()
                    .map(|p| ((*p - circ.center).len() - circ.r).abs())
                    .fold(0.0, f64::max);
                if circ.r > 3.0 && circ.r < 5000.0 && err <= MAX_MODEL_ERR && err < line_err {
                    return Some(Model::Circle {
                        c: circ.center,
                        r: circ.r,
                    });
                }
            }
            if line_err <= MAX_MODEL_ERR {
                return Some(Model::Line { c, n });
            }
        }
        length *= 0.5;
    }
    None
}

pub fn refine_nodes(
    chains: &mut [ChainGeom],
    nodes: &mut [Point],
    used: &[bool],
    width: usize,
    height: usize,
) {
    let mut models: Vec<Vec<Model>> = vec![Vec::new(); nodes.len()];
    for (ch, &u) in chains.iter().zip(used) {
        if !u || ch.closed || ch.start_node == NO_NODE || ch.points.len() < 4 {
            continue;
        }
        if let Some(m) = model_edge(&ch.points) {
            models[ch.start_node as usize].push(m);
        }
        let rev: Vec<Point> = ch.points.iter().rev().copied().collect();
        if let Some(m) = model_edge(&rev) {
            models[ch.end_node as usize].push(m);
        }
    }

    let (w, h) = (width as f64, height as f64);
    for (k, ms) in models.iter().enumerate() {
        if ms.len() < 2 {
            continue;
        }
        let p0 = nodes[k];
        let mut p = p0;
        for _ in 0..8 {
            let (mut axx, mut axy, mut ayy) = (PRIOR, 0.0, PRIOR);
            let (mut bx, mut by) = (-PRIOR * (p.x - p0.x), -PRIOR * (p.y - p0.y));
            for m in ms {
                let (d, g) = m.eval(p);
                axx += g.x * g.x;
                axy += g.x * g.y;
                ayy += g.y * g.y;
                bx -= g.x * d;
                by -= g.y * d;
            }
            let det = axx * ayy - axy * axy;
            if det.abs() < 1e-12 {
                break;
            }
            let delta = Point::new((bx * ayy - by * axy) / det, (axx * by - axy * bx) / det);
            p = p + delta;
            if delta.len() < 1e-6 {
                break;
            }
        }
        // Nodes on the canvas border may only slide along it.
        if p0.x <= 0.0 || p0.x >= w {
            p.x = p0.x;
        }
        if p0.y <= 0.0 || p0.y >= h {
            p.y = p0.y;
        }
        // Trust the move only if every incident edge model agrees with the new position;
        // otherwise fall back to a small, conservative correction.
        let worst = ms.iter().map(|m| m.eval(p).0.abs()).fold(0.0, f64::max);
        let limit = if worst <= AGREE_TOL {
            MAX_SHIFT_TRUSTED
        } else {
            MAX_SHIFT
        };
        let shift = p - p0;
        if shift.len() > limit {
            p = p0 + shift.normalized() * limit;
        }
        if p.is_finite() {
            nodes[k] = p;
        }
    }

    for ch in chains.iter_mut() {
        if !ch.closed && ch.start_node != NO_NODE && ch.points.len() >= 2 {
            let last = ch.points.len() - 1;
            ch.points[0] = nodes[ch.start_node as usize];
            ch.points[last] = nodes[ch.end_node as usize];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_models_find_their_crossing() {
        let a = Model::Line {
            c: Point::new(0.0, 10.0),
            n: Point::new(0.0, 1.0),
        };
        let b = Model::Line {
            c: Point::new(5.0, 0.0),
            n: Point::new(1.0, 0.0),
        };
        let mut p = Point::new(4.0, 11.0);
        for _ in 0..4 {
            let (da, ga) = a.eval(p);
            let (db, gb) = b.eval(p);
            p = Point::new(p.x - gb.x * db, p.y - ga.y * da);
        }
        assert!(p.dist(Point::new(5.0, 10.0)) < 1e-9);
    }

    #[test]
    fn straight_stretch_is_modelled_as_line() {
        let pts: Vec<Point> = (0..30)
            .map(|i| Point::new(i as f64, 3.0 + 0.05 * (i % 2) as f64))
            .collect();
        assert!(matches!(model_edge(&pts), Some(Model::Line { .. })));
    }
}
