//! Vector format writers: SVG, DXF (AC1015), PDF and EPS.

pub mod dxf;
pub mod eps;
pub mod pdf;
pub mod svg;

use crate::geom::Point;
use crate::model::VectorDocument;
use crate::primitives::Primitive;
use crate::topology::planar_graph::ContourRef;

pub use dxf::{DxfOptions, DxfUnits};
pub use svg::SvgOptions;

/// Drawing command shared by the path-based exporters.
#[derive(Clone, Copy, Debug)]
pub enum Seg {
    Move(Point),
    Line(Point),
    Cubic(Point, Point, Point),
    Arc {
        a: Point,
        b: Point,
        center: Point,
        r: f64,
        large: bool,
        sweep: bool,
    },
    Circle {
        center: Point,
        r: f64,
        start: Point,
        sweep: bool,
    },
    Close,
}

/// Flatten one contour into drawing commands (starts with `Move`, ends with `Close`).
pub fn contour_segments(doc: &VectorDocument, contour: &[ContourRef]) -> Vec<Seg> {
    let prims = doc.contour_primitives(contour);
    let mut out = Vec::with_capacity(prims.len() + 2);
    if let Some(first) = prims.first() {
        out.push(Seg::Move(first.start()));
    }
    for p in prims {
        out.push(match p {
            Primitive::Line { b, .. } => Seg::Line(b),
            Primitive::Arc {
                a,
                b,
                center,
                r,
                large,
                sweep,
            } => Seg::Arc {
                a,
                b,
                center,
                r,
                large,
                sweep,
            },
            Primitive::Circle {
                center,
                r,
                start,
                sweep,
            } => Seg::Circle {
                center,
                r,
                start,
                sweep,
            },
            Primitive::Cubic { c1, c2, b, .. } => Seg::Cubic(c1, c2, b),
        });
    }
    out.push(Seg::Close);
    out
}

/// Format a number with at most `prec` decimals, trimming trailing zeros.
pub(crate) fn num(v: f64, prec: usize) -> String {
    let mut s = format!("{:.*}", prec, v);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

pub(crate) fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

impl VectorDocument {
    pub fn to_svg(&self, opts: &SvgOptions) -> String {
        svg::write(self, opts)
    }
    pub fn to_dxf(&self, opts: &DxfOptions) -> String {
        dxf::write(self, opts)
    }
    pub fn to_pdf(&self) -> Vec<u8> {
        pdf::write(self)
    }
    pub fn to_eps(&self) -> String {
        eps::write(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(num(1.0, 3), "1");
        assert_eq!(num(1.23456, 3), "1.235");
        assert_eq!(num(-0.0004, 3), "0");
        assert_eq!(num(10.5, 3), "10.5");
        assert_eq!(num(100.0, 0), "100");
    }
}
