//! Implicit half-edge structure on the pixel-corner lattice.
//!
//! Lattice vertex `(x, y)` (0 ≤ x ≤ W, 0 ≤ y ≤ H) has id `y·(W+1) + x`. A half-edge is a
//! directed unit lattice edge, identified by `vertex·4 + dir` with `dir` 0 = N (y−1),
//! 1 = E (x+1), 2 = S (y+1), 3 = W (x−1). It *exists* iff the two pixels it separates carry
//! different labels. By convention the **left** pixel (as seen on screen while travelling
//! along the edge) is the face the half-edge belongs to; its twin is the same edge reversed.

use super::LabelMap;

pub const N: u8 = 0;
pub const E: u8 = 1;
pub const S: u8 = 2;
pub const W: u8 = 3;

const DX: [isize; 4] = [0, 1, 0, -1];
const DY: [isize; 4] = [-1, 0, 1, 0];

pub struct Lattice<'a> {
    pub map: &'a LabelMap,
    /// Lattice row stride `W + 1`.
    pub stride: usize,
}

impl<'a> Lattice<'a> {
    pub fn new(map: &'a LabelMap) -> Self {
        Lattice {
            map,
            stride: map.width + 1,
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.stride * (self.map.height + 1)
    }

    #[inline]
    pub fn vid(&self, x: usize, y: usize) -> u32 {
        (y * self.stride + x) as u32
    }

    #[inline]
    pub fn xy(&self, v: u32) -> (usize, usize) {
        (v as usize % self.stride, v as usize / self.stride)
    }

    /// Labels of the four pixels around lattice vertex `(x, y)`: `[NW, NE, SE, SW]`.
    #[inline]
    pub fn quad(&self, x: usize, y: usize) -> [u16; 4] {
        let (x, y) = (x as isize, y as isize);
        [
            self.map.get(x - 1, y - 1),
            self.map.get(x, y - 1),
            self.map.get(x, y),
            self.map.get(x - 1, y),
        ]
    }

    /// Which of the four outgoing half-edges (N, E, S, W) exist at `(x, y)`.
    #[inline]
    pub fn out_mask(&self, x: usize, y: usize) -> [bool; 4] {
        let q = self.quad(x, y);
        [q[0] != q[1], q[1] != q[2], q[2] != q[3], q[3] != q[0]]
    }

    /// `(left, right)` labels of the half-edge leaving `(x, y)` in direction `d`.
    #[inline]
    pub fn sides(&self, x: usize, y: usize, d: u8) -> (u16, u16) {
        let q = self.quad(x, y);
        let d = d as usize;
        (q[d], q[(d + 1) % 4])
    }

    #[inline]
    pub fn he(v: u32, d: u8) -> u32 {
        v * 4 + d as u32
    }

    #[inline]
    pub fn he_vertex(he: u32) -> u32 {
        he / 4
    }

    #[inline]
    pub fn he_dir(he: u32) -> u8 {
        (he % 4) as u8
    }

    /// Head vertex of a half-edge.
    #[inline]
    pub fn he_to(&self, he: u32) -> u32 {
        let (x, y) = self.xy(Self::he_vertex(he));
        let d = Self::he_dir(he) as usize;
        self.vid((x as isize + DX[d]) as usize, (y as isize + DY[d]) as usize)
    }

    /// The same edge travelled the other way.
    #[inline]
    pub fn twin(&self, he: u32) -> u32 {
        Self::he(self.he_to(he), (Self::he_dir(he) + 2) % 4)
    }

    #[inline]
    pub fn he_sides(&self, he: u32) -> (u16, u16) {
        let (x, y) = self.xy(Self::he_vertex(he));
        self.sides(x, y, Self::he_dir(he))
    }

    /// Pixel index of the face (left) pixel of a half-edge.
    #[inline]
    pub fn left_pixel(&self, he: u32) -> usize {
        let (x, y) = self.xy(Self::he_vertex(he));
        let (x, y) = (x as isize, y as isize);
        let (px, py) = match Self::he_dir(he) {
            N => (x - 1, y - 1),
            E => (x, y - 1),
            S => (x, y),
            _ => (x - 1, y),
        };
        debug_assert!(
            px >= 0 && py >= 0 && (px as usize) < self.map.width && (py as usize) < self.map.height
        );
        py as usize * self.map.width + px as usize
    }

    /// Next half-edge on the boundary of the face on the left of `he`
    /// (left turn preferred, so diagonally touching pixels stay separate regions).
    pub fn next_in_contour(&self, he: u32) -> u32 {
        let w = self.he_to(he);
        let (wx, wy) = self.xy(w);
        let d = Self::he_dir(he);
        let face = self.he_sides(he).0;
        for cand in [(d + 3) % 4, d, (d + 1) % 4] {
            let (l, r) = self.sides(wx, wy, cand);
            if l != r && l == face {
                return Self::he(w, cand);
            }
        }
        unreachable!("open contour in planar label map")
    }

    /// The single continuing half-edge at a degree-2 vertex.
    pub fn continue_chain(&self, he: u32) -> u32 {
        let w = self.he_to(he);
        let (wx, wy) = self.xy(w);
        let back = (Self::he_dir(he) + 2) % 4;
        let mask = self.out_mask(wx, wy);
        for (dd, &present) in mask.iter().enumerate() {
            if present && dd as u8 != back {
                return Self::he(w, dd as u8);
            }
        }
        unreachable!("dangling chain in planar label map")
    }
}
