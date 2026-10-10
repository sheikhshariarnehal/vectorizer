//! Encapsulated PostScript writer.

use super::{contour_segments, num, Seg};
use crate::geom::angle_of;
use crate::model::VectorDocument;
use crate::primitives::arc_angles;
use std::fmt::Write;

const P: usize = 3;

pub fn write(doc: &VectorDocument) -> String {
    let (w, h) = (doc.width, doc.height);
    let mut s = String::with_capacity(4096);
    let _ = writeln!(s, "%!PS-Adobe-3.0 EPSF-3.0");
    let _ = writeln!(s, "%%Creator: vectorizer-core");
    let _ = writeln!(s, "%%BoundingBox: 0 0 {w} {h}");
    let _ = writeln!(s, "%%HiResBoundingBox: 0 0 {w} {h}");
    let _ = writeln!(s, "%%LanguageLevel: 2");
    let _ = writeln!(s, "%%EndComments");
    let _ = writeln!(s, "gsave");
    let _ = writeln!(s, "0 {h} translate 1 -1 scale");
    for shape in &doc.shapes {
        let c = shape.rgb;
        let _ = writeln!(
            s,
            "{} {} {} setrgbcolor",
            num(c[0] as f64 / 255.0, 4),
            num(c[1] as f64 / 255.0, 4),
            num(c[2] as f64 / 255.0, 4)
        );
        let _ = writeln!(s, "newpath");
        for contour in &shape.contours {
            for seg in contour_segments(doc, contour) {
                match seg {
                    Seg::Move(p) => {
                        let _ = writeln!(s, "{} {} moveto", num(p.x, P), num(p.y, P));
                    }
                    Seg::Line(p) => {
                        let _ = writeln!(s, "{} {} lineto", num(p.x, P), num(p.y, P));
                    }
                    Seg::Cubic(c1, c2, p) => {
                        let _ = writeln!(
                            s,
                            "{} {} {} {} {} {} curveto",
                            num(c1.x, P),
                            num(c1.y, P),
                            num(c2.x, P),
                            num(c2.y, P),
                            num(p.x, P),
                            num(p.y, P)
                        );
                    }
                    Seg::Arc {
                        a,
                        b,
                        center,
                        r,
                        large,
                        sweep,
                    } => {
                        let (a0, d) = arc_angles(a, b, center, large, sweep);
                        let _ = writeln!(
                            s,
                            "{} {} {} {} {} {}",
                            num(center.x, P),
                            num(center.y, P),
                            num(r, P),
                            num(a0.to_degrees(), P),
                            num((a0 + d).to_degrees(), P),
                            if sweep { "arc" } else { "arcn" }
                        );
                    }
                    Seg::Circle {
                        center,
                        r,
                        start,
                        sweep,
                    } => {
                        let a0 = angle_of(start - center).to_degrees();
                        let (a1, op) = if sweep {
                            (a0 + 360.0, "arc")
                        } else {
                            (a0 - 360.0, "arcn")
                        };
                        let _ = writeln!(
                            s,
                            "{} {} {} {} {} {}",
                            num(center.x, P),
                            num(center.y, P),
                            num(r, P),
                            num(a0, P),
                            num(a1, P),
                            op
                        );
                    }
                    Seg::Close => {
                        let _ = writeln!(s, "closepath");
                    }
                }
            }
        }
        let _ = writeln!(s, "eofill");
    }
    let _ = writeln!(s, "grestore");
    let _ = writeln!(s, "showpage");
    let _ = writeln!(s, "%%EOF");
    s
}
