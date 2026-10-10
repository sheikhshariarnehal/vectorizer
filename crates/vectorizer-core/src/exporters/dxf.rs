//! AutoCAD DXF writer (release 2000 / `AC1015`).
//!
//! Every palette colour becomes a layer `LAYER_NN_#RRGGBB`; shape outlines are emitted as
//! native `LINE`, `ARC`, `CIRCLE` and `SPLINE` entities. Y is flipped so the drawing is
//! upright in CAD's y-up world.

use super::{num, Seg};
use crate::geom::Point;
use crate::model::VectorDocument;
use crate::primitives::arc_angles;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DxfUnits {
    Unitless,
    Inches,
    Millimeters,
    Centimeters,
}

impl DxfUnits {
    fn insunits(self) -> u32 {
        match self {
            DxfUnits::Unitless => 0,
            DxfUnits::Inches => 1,
            DxfUnits::Millimeters => 4,
            DxfUnits::Centimeters => 5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DxfOptions {
    pub units: DxfUnits,
    /// Drawing units per source pixel (e.g. `0.2646` for 96 DPI → mm).
    pub scale: f64,
}

impl Default for DxfOptions {
    fn default() -> Self {
        DxfOptions {
            units: DxfUnits::Unitless,
            scale: 1.0,
        }
    }
}

struct Out {
    s: String,
    handle: u32,
}

impl Out {
    fn pair(&mut self, code: i32, value: impl std::fmt::Display) {
        let _ = write!(self.s, "{code}\n{value}\n");
    }
    fn next_handle(&mut self) -> String {
        self.handle += 1;
        format!("{:X}", self.handle)
    }
    fn num(&mut self, code: i32, v: f64) {
        let t = num(v, 6);
        self.pair(code, if t.contains('.') { t } else { format!("{t}.0") });
    }
}

/// Nearest of the nine basic AutoCAD Colour Index entries (used as the layer fallback colour).
fn aci(rgb: [u8; 3]) -> u32 {
    const TABLE: [(u32, [i32; 3]); 9] = [
        (1, [255, 0, 0]),
        (2, [255, 255, 0]),
        (3, [0, 255, 0]),
        (4, [0, 255, 255]),
        (5, [0, 0, 255]),
        (6, [255, 0, 255]),
        (7, [255, 255, 255]),
        (8, [128, 128, 128]),
        (9, [192, 192, 192]),
    ];
    let mut best = (7, i32::MAX);
    for (i, c) in TABLE {
        let d: i32 = (0..3).map(|k| (rgb[k] as i32 - c[k]).pow(2)).sum();
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

pub fn write(doc: &VectorDocument, o: &DxfOptions) -> String {
    let sc = o.scale;
    let hh = doc.height as f64;
    let tx = |p: Point| Point::new(p.x * sc, (hh - p.y) * sc);

    // Palette entries actually used.
    let mut used: Vec<usize> = doc.shapes.iter().map(|s| s.palette_index).collect();
    used.sort_unstable();
    used.dedup();
    let layer_name = |pi: usize| {
        let c = doc.palette[pi];
        format!("LAYER_{:02}_#{:02X}{:02X}{:02X}", pi + 1, c[0], c[1], c[2])
    };

    // ---- entities first (handles are needed for HANDSEED) ----------------------------------------
    let mut w = Out {
        s: String::new(),
        handle: 0x20,
    };
    // Reserve well-known handles.
    let (h_model_rec, h_paper_rec) = (0x1Fu32, 0x1Eu32);
    let owner = format!("{:X}", h_model_rec);

    let mut ent = Out {
        s: String::new(),
        handle: 0x1000,
    };
    for shape in &doc.shapes {
        let layer = layer_name(shape.palette_index);
        for contour in &shape.contours {
            let segs = super::contour_segments(doc, contour);
            let mut cur = Point::ZERO;
            for seg in segs {
                match seg {
                    Seg::Move(p) => cur = p,
                    Seg::Line(p) => {
                        entity_header(&mut ent, "LINE", &owner, &layer, "AcDbLine");
                        point3(&mut ent, 10, tx(cur));
                        point3(&mut ent, 11, tx(p));
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
                        let (ca, cb, cc) = (tx(a), tx(b), tx(center));
                        let _ = arc_angles(a, b, center, large, sweep);
                        // After the y-flip a y-down "sweep" arc runs clockwise.
                        let (from, to) = if sweep { (cb, ca) } else { (ca, cb) };
                        let deg = |p: Point| {
                            let mut a = (p.y - cc.y).atan2(p.x - cc.x).to_degrees();
                            if a < 0.0 {
                                a += 360.0;
                            }
                            a
                        };
                        entity_header(&mut ent, "ARC", &owner, &layer, "AcDbCircle");
                        point3(&mut ent, 10, cc);
                        ent.num(40, r * sc);
                        ent.pair(100, "AcDbArc");
                        ent.num(50, deg(from));
                        ent.num(51, deg(to));
                        cur = b;
                    }
                    Seg::Circle {
                        center, r, start, ..
                    } => {
                        entity_header(&mut ent, "CIRCLE", &owner, &layer, "AcDbCircle");
                        point3(&mut ent, 10, tx(center));
                        ent.num(40, r * sc);
                        cur = start;
                    }
                    Seg::Cubic(c1, c2, p) => {
                        entity_header(&mut ent, "SPLINE", &owner, &layer, "AcDbSpline");
                        ent.num(210, 0.0);
                        ent.num(220, 0.0);
                        ent.num(230, 1.0);
                        ent.pair(70, 8); // planar
                        ent.pair(71, 3); // degree
                        ent.pair(72, 8); // knots
                        ent.pair(73, 4); // control points
                        ent.pair(74, 0); // fit points
                        for k in [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0] {
                            ent.num(40, k);
                        }
                        for q in [cur, c1, c2, p] {
                            point3(&mut ent, 10, tx(q));
                        }
                        cur = p;
                    }
                    Seg::Close => {}
                }
            }
        }
    }

    // ---- header ----------------------------------------------------------------------------------
    let ext = Point::new(doc.width as f64 * sc, doc.height as f64 * sc);
    let handseed = ent.handle + 1;
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1015");
    w.pair(9, "$HANDSEED");
    w.pair(5, format!("{:X}", handseed));
    w.pair(9, "$INSBASE");
    point3(&mut w, 10, Point::ZERO);
    w.pair(9, "$EXTMIN");
    point3(&mut w, 10, Point::ZERO);
    w.pair(9, "$EXTMAX");
    point3(&mut w, 10, ext);
    w.pair(9, "$INSUNITS");
    w.pair(70, o.units.insunits());
    w.pair(9, "$MEASUREMENT");
    w.pair(70, 1);
    w.pair(0, "ENDSEC");

    w.pair(0, "SECTION");
    w.pair(2, "CLASSES");
    w.pair(0, "ENDSEC");

    // ---- tables ----------------------------------------------------------------------------------
    w.pair(0, "SECTION");
    w.pair(2, "TABLES");
    empty_table(&mut w, "VPORT", "0");
    // LTYPE
    let h = w.next_handle();
    table_start(&mut w, "LTYPE", &h, 3);
    for (name, desc) in [
        ("ByBlock", ""),
        ("ByLayer", ""),
        ("Continuous", "Solid line"),
    ] {
        let hr = w.next_handle();
        w.pair(0, "LTYPE");
        w.pair(5, hr);
        w.pair(330, &h);
        w.pair(100, "AcDbSymbolTableRecord");
        w.pair(100, "AcDbLinetypeTableRecord");
        w.pair(2, name);
        w.pair(70, 0);
        w.pair(3, desc);
        w.pair(72, 65);
        w.pair(73, 0);
        w.num(40, 0.0);
    }
    w.pair(0, "ENDTAB");
    // LAYER
    let h = w.next_handle();
    table_start(&mut w, "LAYER", &h, used.len() + 1);
    let layer_rec = |w: &mut Out, tbl: &str, name: &str, rgb: [u8; 3], truecolor: bool| {
        let hr = w.next_handle();
        w.pair(0, "LAYER");
        w.pair(5, hr);
        w.pair(330, tbl);
        w.pair(100, "AcDbSymbolTableRecord");
        w.pair(100, "AcDbLayerTableRecord");
        w.pair(2, name);
        w.pair(70, 0);
        w.pair(62, aci(rgb));
        if truecolor {
            w.pair(
                420,
                ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32,
            );
        }
        w.pair(6, "Continuous");
    };
    layer_rec(&mut w, &h, "0", [255, 255, 255], false);
    for &pi in &used {
        let name = layer_name(pi);
        layer_rec(&mut w, &h, &name, doc.palette[pi], true);
    }
    w.pair(0, "ENDTAB");
    // STYLE
    let h = w.next_handle();
    table_start(&mut w, "STYLE", &h, 1);
    let hr = w.next_handle();
    w.pair(0, "STYLE");
    w.pair(5, hr);
    w.pair(330, &h);
    w.pair(100, "AcDbSymbolTableRecord");
    w.pair(100, "AcDbTextStyleTableRecord");
    w.pair(2, "Standard");
    w.pair(70, 0);
    w.num(40, 0.0);
    w.num(41, 1.0);
    w.num(50, 0.0);
    w.pair(71, 0);
    w.num(42, 2.5);
    w.pair(3, "txt");
    w.pair(4, "");
    w.pair(0, "ENDTAB");
    empty_table(&mut w, "VIEW", "0");
    empty_table(&mut w, "UCS", "0");
    // APPID
    let h = w.next_handle();
    table_start(&mut w, "APPID", &h, 1);
    let hr = w.next_handle();
    w.pair(0, "APPID");
    w.pair(5, hr);
    w.pair(330, &h);
    w.pair(100, "AcDbSymbolTableRecord");
    w.pair(100, "AcDbRegAppTableRecord");
    w.pair(2, "ACAD");
    w.pair(70, 0);
    w.pair(0, "ENDTAB");
    // DIMSTYLE
    let h = w.next_handle();
    w.pair(0, "TABLE");
    w.pair(2, "DIMSTYLE");
    w.pair(5, &h);
    w.pair(330, 0);
    w.pair(100, "AcDbSymbolTable");
    w.pair(70, 0);
    w.pair(100, "AcDbDimStyleTable");
    w.pair(71, 0);
    w.pair(0, "ENDTAB");
    // BLOCK_RECORD
    let h = w.next_handle();
    table_start(&mut w, "BLOCK_RECORD", &h, 2);
    for (hr, name) in [(h_model_rec, "*Model_Space"), (h_paper_rec, "*Paper_Space")] {
        w.pair(0, "BLOCK_RECORD");
        w.pair(5, format!("{:X}", hr));
        w.pair(330, &h);
        w.pair(100, "AcDbSymbolTableRecord");
        w.pair(100, "AcDbBlockTableRecord");
        w.pair(2, name);
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "ENDSEC");

    // ---- blocks ----------------------------------------------------------------------------------
    w.pair(0, "SECTION");
    w.pair(2, "BLOCKS");
    for (hr, name, paper) in [
        (h_model_rec, "*Model_Space", false),
        (h_paper_rec, "*Paper_Space", true),
    ] {
        let hb = w.next_handle();
        w.pair(0, "BLOCK");
        w.pair(5, hb);
        w.pair(330, format!("{:X}", hr));
        w.pair(100, "AcDbEntity");
        if paper {
            w.pair(67, 1);
        }
        w.pair(8, "0");
        w.pair(100, "AcDbBlockBegin");
        w.pair(2, name);
        w.pair(70, 0);
        point3(&mut w, 10, Point::ZERO);
        w.pair(3, name);
        w.pair(1, "");
        let he = w.next_handle();
        w.pair(0, "ENDBLK");
        w.pair(5, he);
        w.pair(330, format!("{:X}", hr));
        w.pair(100, "AcDbEntity");
        if paper {
            w.pair(67, 1);
        }
        w.pair(8, "0");
        w.pair(100, "AcDbBlockEnd");
    }
    w.pair(0, "ENDSEC");

    // ---- entities ----------------------------------------------------------------------------------
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    w.s.push_str(&ent.s);
    w.pair(0, "ENDSEC");

    // ---- objects -----------------------------------------------------------------------------------
    w.pair(0, "SECTION");
    w.pair(2, "OBJECTS");
    let hd = w.next_handle();
    w.pair(0, "DICTIONARY");
    w.pair(5, hd);
    w.pair(330, 0);
    w.pair(100, "AcDbDictionary");
    w.pair(281, 1);
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    w.s
}

fn table_start(w: &mut Out, name: &str, handle: &str, count: usize) {
    w.pair(0, "TABLE");
    w.pair(2, name);
    w.pair(5, handle);
    w.pair(330, 0);
    w.pair(100, "AcDbSymbolTable");
    w.pair(70, count);
}

fn empty_table(w: &mut Out, name: &str, _owner: &str) {
    let h = w.next_handle();
    table_start(w, name, &h, 0);
    w.pair(0, "ENDTAB");
}

fn entity_header(w: &mut Out, kind: &str, owner: &str, layer: &str, subclass: &str) {
    let h = w.next_handle();
    w.pair(0, kind);
    w.pair(5, h);
    w.pair(330, owner);
    w.pair(100, "AcDbEntity");
    w.pair(8, layer);
    w.pair(100, subclass);
}

fn point3(w: &mut Out, code: i32, p: Point) {
    w.num(code, p.x);
    w.num(code + 10, p.y);
    w.num(code + 20, 0.0);
}
