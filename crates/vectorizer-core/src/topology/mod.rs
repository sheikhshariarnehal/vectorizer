//! Watertight planar-map topology.
//!
//! The quantised image is a label map. Region boundaries live on the pixel-corner lattice
//! and form a planar graph in which every boundary is stored exactly **once** and is shared
//! by the faces on both sides (half-edge / twin relation). Primitives are fitted once per
//! shared chain, so neighbouring shapes can never leave a gap or overlap.

pub mod half_edge;
pub mod planar_graph;
pub mod speckle;
pub mod stacking;

/// Label of fully transparent pixels (never emitted as a shape).
pub const TRANSPARENT: u16 = 0xFFFE;
/// Label of everything outside the image canvas.
pub const OUTSIDE: u16 = 0xFFFF;

/// Row-major map of palette indices (or [`TRANSPARENT`]).
#[derive(Clone, Debug)]
pub struct LabelMap {
    pub width: usize,
    pub height: usize,
    pub labels: Vec<u16>,
}

impl LabelMap {
    pub fn new(width: usize, height: usize, labels: Vec<u16>) -> Self {
        assert_eq!(labels.len(), width * height);
        LabelMap {
            width,
            height,
            labels,
        }
    }

    /// Label at `(x, y)`; [`OUTSIDE`] beyond the canvas.
    #[inline]
    pub fn get(&self, x: isize, y: isize) -> u16 {
        if x < 0 || y < 0 || x >= self.width as isize || y >= self.height as isize {
            OUTSIDE
        } else {
            self.labels[y as usize * self.width + x as usize]
        }
    }

    /// True if the label is rendered (i.e. a real palette colour).
    #[inline]
    pub fn is_paintable(label: u16) -> bool {
        label != TRANSPARENT && label != OUTSIDE
    }
}
