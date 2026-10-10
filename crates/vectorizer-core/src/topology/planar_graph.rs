//! Watertight boundary-graph builder: label map → shared chains + faces.

use super::half_edge::Lattice;
use super::speckle::Components;
use super::LabelMap;
use crate::geom::Point;
use crate::subpixel::{marching_squares::edge_point, CoverageField};
use rayon::prelude::*;
use std::collections::HashMap;

pub const NO_NODE: u32 = u32::MAX;
const NONE: u32 = u32::MAX;

/// A boundary between two labels, running node → node (or a closed loop with no node).
/// `left`/`right` are the labels on either side of the chain's canonical direction
/// (screen-left / screen-right while travelling).
#[derive(Clone, Debug)]
pub struct ChainGeom {
    pub left: u16,
    pub right: u16,
    pub closed: bool,
    /// Sub-pixel sample points. For open chains the first and last point are the lattice
    /// positions of the junction nodes.
    pub points: Vec<Point>,
    pub start_node: u32,
    pub end_node: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContourRef {
    pub chain: u32,
    pub reversed: bool,
}

/// One connected region of a single colour: outer contour first, then holes.
#[derive(Clone, Debug)]
pub struct FaceTopo {
    pub label: u16,
    pub contours: Vec<Vec<ContourRef>>,
    /// Lattice area (px²) of each contour.
    pub areas: Vec<f64>,
}

#[derive(Debug)]
pub struct PlanarMap {
    pub chains: Vec<ChainGeom>,
    pub faces: Vec<FaceTopo>,
    /// Junction nodes (lattice positions), shared by every chain that meets there.
    pub nodes: Vec<Point>,
}

struct RawChain {
    start: usize,
    end: usize,
    closed: bool,
    area2: f64,
}

pub fn build(map: &LabelMap, field: &CoverageField, comps: &Components) -> PlanarMap {
    let lat = Lattice::new(map);
    let (w, h) = (map.width, map.height);
    let nv = lat.vertex_count();

    // --- 1. classify lattice vertices -------------------------------------------------------
    let mut is_node = vec![false; nv];
    let mut node_list: Vec<u32> = Vec::new();
    for y in 0..=h {
        for x in 0..=w {
            let m = lat.out_mask(x, y);
            if m.iter().filter(|&&b| b).count() >= 3 {
                let v = lat.vid(x, y);
                is_node[v as usize] = true;
                node_list.push(v);
            }
        }
    }

    // --- 2. trace chains ---------------------------------------------------------------------
    let mut assigned = vec![NONE; nv * 4];
    let mut pool: Vec<u32> = Vec::new();
    let mut raw: Vec<RawChain> = Vec::new();

    let trace =
        |start: u32, assigned: &mut Vec<u32>, pool: &mut Vec<u32>, raw: &mut Vec<RawChain>| {
            let cidx = raw.len() as u32;
            let begin = pool.len();
            let mut cur = start;
            let mut area2 = 0.0f64;
            let closed;
            loop {
                pool.push(cur);
                assigned[cur as usize] = cidx << 1;
                assigned[lat.twin(cur) as usize] = (cidx << 1) | 1;
                let (x0, y0) = lat.xy(Lattice::he_vertex(cur));
                let (x1, y1) = lat.xy(lat.he_to(cur));
                area2 += x0 as f64 * y1 as f64 - x1 as f64 * y0 as f64;
                let wv = lat.he_to(cur);
                if is_node[wv as usize] {
                    closed = false;
                    break;
                }
                let next = lat.continue_chain(cur);
                if next == start {
                    closed = true;
                    break;
                }
                cur = next;
            }
            raw.push(RawChain {
                start: begin,
                end: pool.len(),
                closed,
                area2,
            });
        };

    for &v in &node_list {
        let (x, y) = lat.xy(v);
        let m = lat.out_mask(x, y);
        for d in 0..4u8 {
            if m[d as usize] {
                let he = Lattice::he(v, d);
                if assigned[he as usize] == NONE {
                    trace(he, &mut assigned, &mut pool, &mut raw);
                }
            }
        }
    }
    for y in 0..=h {
        for x in 0..=w {
            let m = lat.out_mask(x, y);
            if !(m[0] || m[1] || m[2] || m[3]) {
                continue;
            }
            let v = lat.vid(x, y);
            for d in 0..4u8 {
                if m[d as usize] {
                    let he = Lattice::he(v, d);
                    if assigned[he as usize] == NONE {
                        trace(he, &mut assigned, &mut pool, &mut raw);
                    }
                }
            }
        }
    }

    // --- 3. sub-pixel geometry (parallel over chains) ------------------------------------------
    let node_of = |v: u32| -> u32 {
        node_list
            .binary_search(&v)
            .map(|i| i as u32)
            .unwrap_or(NO_NODE)
    };
    let chains: Vec<ChainGeom> = raw
        .par_iter()
        .map(|rc| {
            let hes = &pool[rc.start..rc.end];
            let first = hes[0];
            let (left, right) = lat.he_sides(first);
            let mut points = Vec::with_capacity(hes.len() + 2);
            let (sx, sy) = lat.xy(Lattice::he_vertex(first));
            if !rc.closed {
                points.push(Point::new(sx as f64, sy as f64));
            }
            for &he in hes {
                let (x0, y0) = lat.xy(Lattice::he_vertex(he));
                let (x1, y1) = lat.xy(lat.he_to(he));
                points.push(edge_point(field, map, x0, y0, x1, y1));
            }
            let (start_node, end_node) = if rc.closed {
                (NO_NODE, NO_NODE)
            } else {
                let last_to = lat.he_to(*hes.last().unwrap());
                let (ex, ey) = lat.xy(last_to);
                points.push(Point::new(ex as f64, ey as f64));
                (node_of(Lattice::he_vertex(first)), node_of(last_to))
            };
            ChainGeom {
                left,
                right,
                closed: rc.closed,
                points,
                start_node,
                end_node,
            }
        })
        .collect();

    let nodes: Vec<Point> = node_list
        .iter()
        .map(|&v| {
            let (x, y) = lat.xy(v);
            Point::new(x as f64, y as f64)
        })
        .collect();

    // --- 4. contours & faces ---------------------------------------------------------------
    let n = raw.len();
    let mut visited = vec![false; n * 2];
    let mut face_of_comp: HashMap<u32, usize> = HashMap::new();
    let mut faces: Vec<FaceTopo> = Vec::new();
    let mut pending: Vec<Vec<(Vec<ContourRef>, f64)>> = Vec::new();

    let first_he = |c: usize, rev: bool| -> u32 {
        if !rev {
            pool[raw[c].start]
        } else {
            lat.twin(pool[raw[c].end - 1])
        }
    };
    let last_he = |c: usize, rev: bool| -> u32 {
        if !rev {
            pool[raw[c].end - 1]
        } else {
            lat.twin(pool[raw[c].start])
        }
    };

    for c in 0..n {
        for rev in [false, true] {
            let face_label = if rev { chains[c].right } else { chains[c].left };
            if !LabelMap::is_paintable(face_label) || visited[c * 2 + rev as usize] {
                continue;
            }
            let mut refs: Vec<ContourRef> = Vec::new();
            let mut area2 = 0.0;
            let (mut cc, mut cr) = (c, rev);
            let mut guard = 0usize;
            loop {
                visited[cc * 2 + cr as usize] = true;
                refs.push(ContourRef {
                    chain: cc as u32,
                    reversed: cr,
                });
                area2 += if cr { -raw[cc].area2 } else { raw[cc].area2 };
                if raw[cc].closed {
                    break;
                }
                let nh = lat.next_in_contour(last_he(cc, cr));
                let a = assigned[nh as usize];
                let (nc, nr) = ((a >> 1) as usize, a & 1 == 1);
                if (nc, nr) == (c, rev) {
                    break;
                }
                cc = nc;
                cr = nr;
                guard += 1;
                if guard > 2 * n + 2 {
                    break;
                }
            }
            let comp = comps.id[lat.left_pixel(first_he(c, rev))];
            let fi = *face_of_comp.entry(comp).or_insert_with(|| {
                faces.push(FaceTopo {
                    label: face_label,
                    contours: Vec::new(),
                    areas: Vec::new(),
                });
                pending.push(Vec::new());
                faces.len() - 1
            });
            pending[fi].push((refs, area2));
        }
    }

    for (face, mut cs) in faces.iter_mut().zip(pending) {
        // Outer boundary has negative signed area in this orientation; holes are positive.
        cs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for (refs, a2) in cs {
            face.contours.push(refs);
            face.areas.push(a2.abs() * 0.5);
        }
    }

    PlanarMap {
        chains,
        faces,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subpixel::CoverageField;
    use crate::topology::speckle::components;

    fn field_for<'a>(w: usize, h: usize, rgba: &'a [u8], pal: &'a [[f32; 3]]) -> CoverageField<'a> {
        CoverageField {
            width: w,
            height: h,
            rgba,
            palette: pal,
        }
    }

    /// For every paintable label the signed contour areas must add up to the pixel count.
    fn check_map(w: usize, h: usize, labels: Vec<u16>) {
        let map = LabelMap::new(w, h, labels.clone());
        let rgba = vec![255u8; w * h * 4];
        let pal = vec![[0.0f32; 3], [255.0, 0.0, 0.0], [0.0, 255.0, 0.0]];
        let field = field_for(w, h, &rgba, &pal);
        let comps = components(&map);
        let pm = build(&map, &field, &comps);
        for label in 0..3u16 {
            let px = labels.iter().filter(|&&l| l == label).count() as f64;
            let mut total = 0.0;
            for f in pm.faces.iter().filter(|f| f.label == label) {
                // outer first
                for (i, a) in f.areas.iter().enumerate() {
                    total += if i == 0 { *a } else { -*a };
                }
            }
            assert!(
                (total - px).abs() < 1e-9,
                "label {label}: contour area {total} != pixels {px}\n{labels:?}"
            );
        }
    }

    #[test]
    fn single_pixel_and_rect() {
        check_map(3, 3, vec![0, 0, 0, 0, 1, 0, 0, 0, 0]);
        check_map(4, 3, vec![0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn checkerboard_and_holes() {
        check_map(4, 4, vec![0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0]);
        check_map(
            5,
            5,
            vec![
                1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 0, 2, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1,
            ],
        );
    }

    #[test]
    fn pseudo_random_maps() {
        let mut s = 12345u64;
        for _ in 0..200 {
            let (w, h) = (7, 6);
            let labels: Vec<u16> = (0..w * h)
                .map(|_| {
                    s = s
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    ((s >> 33) % 3) as u16
                })
                .collect();
            check_map(w, h, labels);
        }
    }

    #[test]
    fn shared_chains_are_used_by_both_sides() {
        let (w, h) = (4, 2);
        let labels = vec![0, 0, 1, 1, 0, 0, 1, 1];
        let map = LabelMap::new(w, h, labels);
        let rgba = vec![255u8; w * h * 4];
        let pal = vec![[0.0f32; 3], [255.0, 0.0, 0.0]];
        let field = field_for(w, h, &rgba, &pal);
        let pm = build(&map, &field, &components(&map));
        assert_eq!(pm.faces.len(), 2);
        let used = |face: usize| -> Vec<u32> {
            let mut v: Vec<u32> = pm.faces[face].contours[0].iter().map(|r| r.chain).collect();
            v.sort();
            v
        };
        let (a, b) = (used(0), used(1));
        let shared: Vec<_> = a.iter().filter(|c| b.contains(c)).collect();
        assert_eq!(shared.len(), 1, "exactly one shared chain expected");
    }
}
