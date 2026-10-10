//! The vector document produced by the engine and consumed by the exporters.

use crate::geom::Point;
use crate::primitives::{Primitive, PrimitiveKind};
use crate::topology::planar_graph::ContourRef;
use crate::types::TopologyMode;

/// A shared boundary between two colour regions, fitted once.
#[derive(Clone, Debug)]
pub struct VectorChain {
    /// Palette index of the region on the left / right (may be transparent / outside).
    pub left: u16,
    pub right: u16,
    pub closed: bool,
    /// Connected primitives in canonical direction (empty if no shape references the chain).
    pub prims: Vec<Primitive>,
}

/// One filled region: outer contour first, then holes (cutout mode only).
#[derive(Clone, Debug)]
pub struct Shape {
    pub palette_index: usize,
    pub rgb: [u8; 3],
    pub contours: Vec<Vec<ContourRef>>,
    /// Area of the outer contour in px².
    pub area: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StageTimings {
    pub prepare_ms: f64,
    pub quantize_ms: f64,
    pub topology_ms: f64,
    pub fit_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub width: u32,
    pub height: u32,
    pub palette_count: usize,
    pub shapes: usize,
    pub chains: usize,
    pub lines: usize,
    pub arcs: usize,
    pub circles: usize,
    pub beziers: usize,
    /// Corner vertices + multi-colour junction nodes.
    pub nodes: usize,
    pub timings: StageTimings,
}

#[derive(Clone, Debug)]
pub struct VectorDocument {
    pub width: u32,
    pub height: u32,
    pub topology: TopologyMode,
    pub palette: Vec<[u8; 3]>,
    pub chains: Vec<VectorChain>,
    /// Shapes in painting order (bottom first).
    pub shapes: Vec<Shape>,
    /// Multi-colour junction vertices.
    pub junctions: Vec<Point>,
    /// Sharp corner vertices inside chains.
    pub corners: Vec<Point>,
    pub stats: Stats,
}

impl VectorDocument {
    /// The primitives of one contour in drawing order, each oriented along the contour.
    pub fn contour_primitives(&self, contour: &[ContourRef]) -> Vec<Primitive> {
        let mut out = Vec::new();
        for r in contour {
            let chain = &self.chains[r.chain as usize];
            if r.reversed {
                out.extend(chain.prims.iter().rev().map(|p| p.reversed()));
            } else {
                out.extend(chain.prims.iter().copied());
            }
        }
        out
    }

    /// Count of primitives of `kind` across all chains.
    pub fn count(&self, kind: PrimitiveKind) -> usize {
        self.chains
            .iter()
            .flat_map(|c| &c.prims)
            .filter(|p| p.kind() == kind)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}
