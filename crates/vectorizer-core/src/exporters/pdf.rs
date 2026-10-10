//! Minimal, dependency-free PDF 1.4 writer (single page, vector fills only).

use super::{contour_segments, num, Seg};
use crate::geom::{arc_to_cubics, Point};
use crate::model::VectorDocument;
use crate::primitives::arc_angles;
use std::fmt::Write;

const P: usize = 3;

fn content_stream(doc: &VectorDocument) -> String {
    let mut c = String::with_capacity(4096);
    // Flip to a y-down coordinate system matching the raster.
    let _ = writeln!(c, "1 0 0 -1 0 {} cm", doc.height);
    for shape in &doc.shapes {
        let rgb = shape.rgb;
        let _ = writeln!(
            c,
            "{} {} {} rg",
            num(rgb[0] as f64 / 255.0, 4),
            num(rgb[1] as f64 / 255.0, 4),
            num(rgb[2] as f64 / 255.0, 4)
        );
        for contour in &shape.contours {
            let mut cur = Point::ZERO;
            for seg in contour_segments(doc, contour) {
                match seg {
                    Seg::Move(p) => {
                        let _ = writeln!(c, "{} {} m", num(p.x, P), num(p.y, P));
                        cur = p;
                    }
                    Seg::Line(p) => {
                        let _ = writeln!(c, "{} {} l", num(p.x, P), num(p.y, P));
                        cur = p;
                    }
                    Seg::Cubic(c1, c2, p) => {
                        let _ = writeln!(
                            c,
                            "{} {} {} {} {} {} c",
                            num(c1.x, P),
                            num(c1.y, P),
                            num(c2.x, P),
                            num(c2.y, P),
                            num(p.x, P),
                            num(p.y, P)
                        );
                        cur = p;
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
                        for q in arc_to_cubics(center, r, a0, d) {
                            cubic(&mut c, q);
                        }
                        cur = b;
                    }
                    Seg::Circle {
                        center,
                        r,
                        start,
                        sweep,
                    } => {
                        let a0 = (start.y - center.y).atan2(start.x - center.x);
                        let sw = if sweep {
                            std::f64::consts::TAU
                        } else {
                            -std::f64::consts::TAU
                        };
                        for q in arc_to_cubics(center, r, a0, sw) {
                            cubic(&mut c, q);
                        }
                        cur = start;
                    }
                    Seg::Close => {
                        let _ = writeln!(c, "h");
                    }
                }
            }
            let _ = cur;
        }
        let _ = writeln!(c, "f*");
    }
    c
}

fn cubic(c: &mut String, q: [Point; 4]) {
    let _ = writeln!(
        c,
        "{} {} {} {} {} {} c",
        num(q[1].x, P),
        num(q[1].y, P),
        num(q[2].x, P),
        num(q[2].y, P),
        num(q[3].x, P),
        num(q[3].y, P)
    );
}

pub fn write(doc: &VectorDocument) -> Vec<u8> {
    let content = content_stream(doc);
    let mut out: Vec<u8> = Vec::with_capacity(content.len() + 1024);
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut out,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << >> >>",
            doc.width, doc.height
        )
        .as_bytes(),
    );
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content.as_bytes());
    stream.extend_from_slice(b"endstream");
    obj(&mut out, &stream);

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for o in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", o).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}
