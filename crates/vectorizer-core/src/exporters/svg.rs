//! Clean semantic SVG writer.

use super::{contour_segments, hex, num, Seg};
use crate::model::VectorDocument;
use crate::primitives::Primitive;
use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct SvgOptions {
    /// Decimal places (3 by default).
    pub precision: usize,
    /// Emit an extra `<g id="primitives">` overlay with one element per primitive carrying
    /// the semantic classes `v-line`, `v-arc`, `v-bezier` plus `v-node` markers.
    pub primitive_overlay: bool,
    /// Add a same-colour hairline stroke to hide renderer anti-aliasing seams between
    /// adjacent shapes (0 disables). Geometry itself is always seam-free.
    pub seam_stroke: f64,
    /// Emit a native `<circle>` for shapes that are exactly one circle.
    pub native_circles: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            precision: 3,
            primitive_overlay: false,
            seam_stroke: 0.0,
            native_circles: true,
        }
    }
}

fn path_data(
    doc: &VectorDocument,
    contour: &[crate::topology::planar_graph::ContourRef],
    prec: usize,
) -> String {
    let mut d = String::new();
    let pt = |p: crate::geom::Point| format!("{} {}", num(p.x, prec), num(p.y, prec));
    for seg in contour_segments(doc, contour) {
        match seg {
            Seg::Move(p) => {
                let _ = write!(d, "M{}", pt(p));
            }
            Seg::Line(p) => {
                let _ = write!(d, "L{}", pt(p));
            }
            Seg::Cubic(c1, c2, p) => {
                let _ = write!(d, "C{} {} {}", pt(c1), pt(c2), pt(p));
            }
            Seg::Arc {
                b, r, large, sweep, ..
            } => {
                let _ = write!(
                    d,
                    "A{} {} 0 {} {} {}",
                    num(r, prec),
                    num(r, prec),
                    large as u8,
                    sweep as u8,
                    pt(b)
                );
            }
            Seg::Circle {
                center,
                r,
                start,
                sweep,
            } => {
                let opposite = center * 2.0 - start;
                let rr = num(r, prec);
                let _ = write!(d, "A{rr} {rr} 0 0 {} {}", sweep as u8, pt(opposite));
                let _ = write!(d, "A{rr} {rr} 0 0 {} {}", sweep as u8, pt(start));
            }
            Seg::Close => d.push('Z'),
        }
    }
    d
}

pub fn write(doc: &VectorDocument, o: &SvgOptions) -> String {
    let prec = o.precision;
    let mut s = String::with_capacity(4096);
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        w = doc.width,
        h = doc.height
    );
    let _ = writeln!(s, "<g id=\"shapes\">");
    for shape in &doc.shapes {
        let fill = hex(shape.rgb);
        let stroke = if o.seam_stroke > 0.0 {
            format!(
                " stroke=\"{fill}\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
                num(o.seam_stroke, 3)
            )
        } else {
            String::new()
        };
        // Native <circle> for a lone, hole-free circle.
        if o.native_circles && shape.contours.len() == 1 && shape.contours[0].len() == 1 {
            let chain = &doc.chains[shape.contours[0][0].chain as usize];
            if let [Primitive::Circle { center, r, .. }] = chain.prims.as_slice() {
                let _ = writeln!(
                    s,
                    "<circle class=\"v-arc\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\"{stroke}/>",
                    num(center.x, prec),
                    num(center.y, prec),
                    num(*r, prec)
                );
                continue;
            }
        }
        let mut d = String::new();
        for c in &shape.contours {
            d.push_str(&path_data(doc, c, prec));
        }
        let rule = if shape.contours.len() > 1 {
            " fill-rule=\"evenodd\""
        } else {
            ""
        };
        let _ = writeln!(s, "<path d=\"{d}\" fill=\"{fill}\"{rule}{stroke}/>");
    }
    let _ = writeln!(s, "</g>");

    if o.primitive_overlay {
        let _ = writeln!(s, "<g id=\"primitives\" fill=\"none\" stroke-width=\"0.5\" vector-effect=\"non-scaling-stroke\">");
        for chain in &doc.chains {
            for p in &chain.prims {
                write_overlay_primitive(&mut s, p, prec);
            }
        }
        for n in doc.junctions.iter().chain(&doc.corners) {
            let _ = writeln!(
                s,
                "<circle class=\"v-node\" cx=\"{}\" cy=\"{}\" r=\"1\" fill=\"#FF1744\"/>",
                num(n.x, prec),
                num(n.y, prec)
            );
        }
        let _ = writeln!(s, "</g>");
    }
    s.push_str("</svg>\n");
    s
}

fn write_overlay_primitive(s: &mut String, p: &Primitive, prec: usize) {
    let pt = |q: crate::geom::Point| format!("{} {}", num(q.x, prec), num(q.y, prec));
    match *p {
        Primitive::Line { a, b } => {
            let _ = writeln!(
                s,
                "<path class=\"v-line\" stroke=\"#00BFFF\" d=\"M{}L{}\"/>",
                pt(a),
                pt(b)
            );
        }
        Primitive::Arc {
            a,
            b,
            r,
            large,
            sweep,
            ..
        } => {
            let _ = writeln!(
                s,
                "<path class=\"v-arc\" stroke=\"#00E676\" d=\"M{}A{} {} 0 {} {} {}\"/>",
                pt(a),
                num(r, prec),
                num(r, prec),
                large as u8,
                sweep as u8,
                pt(b)
            );
        }
        Primitive::Circle { center, r, .. } => {
            let _ = writeln!(
                s,
                "<circle class=\"v-arc\" stroke=\"#00E676\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                num(center.x, prec),
                num(center.y, prec),
                num(r, prec)
            );
        }
        Primitive::Cubic { a, c1, c2, b } => {
            let _ = writeln!(
                s,
                "<path class=\"v-bezier\" stroke=\"#FF9100\" d=\"M{}C{} {} {}\"/>",
                pt(a),
                pt(c1),
                pt(c2),
                pt(b)
            );
        }
    }
}
