//! Synthetic anti-aliased scenes and a reference rasteriser of [`VectorDocument`]s.
//!
//! Used by the integration tests (acceptance matrix) and the CLI `demo` command. The scenes
//! are rendered with exact box-filter anti-aliasing (supersampling), i.e. precisely the
//! rasterisation model the engine inverts.

use crate::geom::Point;
use crate::model::VectorDocument;
use crate::topology::planar_graph::ContourRef;
use crate::types::RasterImage;

pub type Rgba = [f32; 4];

/// Render `f` (straight RGBA in 0..1 at a continuous position) with `ss × ss` supersampling.
pub fn render(width: u32, height: u32, ss: u32, f: impl Fn(f64, f64) -> Rgba) -> RasterImage {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (mut r, mut g, mut b, mut a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = x as f64 + (sx as f64 + 0.5) / ss as f64;
                    let py = y as f64 + (sy as f64 + 0.5) / ss as f64;
                    let c = f(px, py);
                    let ca = c[3] as f64;
                    r += c[0] as f64 * ca;
                    g += c[1] as f64 * ca;
                    b += c[2] as f64 * ca;
                    a += ca;
                }
            }
            let n = (ss * ss) as f64;
            let (ur, ug, ub) = if a > 0.0 {
                (r / a, g / a, b / a)
            } else {
                (0.0, 0.0, 0.0)
            };
            data.extend_from_slice(&[
                (ur * 255.0 + 0.5) as u8,
                (ug * 255.0 + 0.5) as u8,
                (ub * 255.0 + 0.5) as u8,
                (a / n * 255.0 + 0.5) as u8,
            ]);
        }
    }
    RasterImage::new(width, height, data).expect("valid synthetic image")
}

const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];
const BLACK: Rgba = [0.0, 0.0, 0.0, 1.0];
const RED: Rgba = [0.86, 0.12, 0.10, 1.0];
const BLUE: Rgba = [0.10, 0.25, 0.75, 1.0];
const YELLOW: Rgba = [0.98, 0.80, 0.10, 1.0];
const GREEN: Rgba = [0.10, 0.60, 0.25, 1.0];
const CLEAR: Rgba = [0.0, 0.0, 0.0, 0.0];

fn dist(x: f64, y: f64, cx: f64, cy: f64) -> f64 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
}

/// Red disc inside a blue ring on white. Radii as fractions of `size`: disc 0.28, ring 0.38–0.45.
pub fn scene_circle_ring(size: u32) -> RasterImage {
    let s = size as f64;
    let c = s / 2.0;
    render(size, size, 8, move |x, y| {
        let d = dist(x, y, c, c);
        if d < 0.28 * s {
            RED
        } else if d > 0.38 * s && d < 0.45 * s {
            BLUE
        } else {
            WHITE
        }
    })
}

/// Black butt-capped strokes on white: a square frame, a 45° diagonal and a band that is
/// 0.6° off horizontal. Sharp corners only (no round caps), like a CAD drawing.
pub fn scene_blueprint(size: u32) -> RasterImage {
    let s = size as f64;
    let t = 1.0f64.max(s / 100.0); // half line width
                                   // band along a -> b with flat ends
    let band = move |p: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = (dx / l, dy / l);
        let (rx, ry) = (p.0 - a.0, p.1 - a.1);
        let along = rx * ux + ry * uy;
        let perp = (rx * uy - ry * ux).abs();
        (0.0..=l).contains(&along) && perp < t
    };
    render(size, size, 8, move |x, y| {
        let p = (x, y);
        let (m, w) = (0.1 * s, 0.9 * s);
        let outer = x > m && x < w && y > m && y < w;
        let inner = x > m + 2.0 * t && x < w - 2.0 * t && y > m + 2.0 * t && y < w - 2.0 * t;
        let frame = outer && !inner;
        let diag = band(p, (m + 2.0 * t, m + 2.0 * t), (0.5 * s, 0.5 * s));
        let tilted = band(p, (0.2 * s, 0.7 * s), (0.8 * s, 0.7 * s + 0.0063 * s));
        if frame || diag || tilted {
            BLACK
        } else {
            WHITE
        }
    })
}

/// Tricolour with a yellow disc and a green triangle: many 3-colour junctions.
pub fn scene_flag(w: u32, h: u32) -> RasterImage {
    let (fw, fh) = (w as f64, h as f64);
    render(w, h, 8, move |x, y| {
        let tri = x < 0.3 * fw && (y - fh / 2.0).abs() < (0.3 * fw - x) * 0.9;
        if tri {
            GREEN
        } else if dist(x, y, 0.62 * fw, 0.5 * fh) < 0.18 * fh {
            YELLOW
        } else if y < fh / 3.0 {
            RED
        } else if y < 2.0 * fh / 3.0 {
            WHITE
        } else {
            BLUE
        }
    })
}

/// Opaque discs on a fully transparent background (alpha anti-aliasing only).
pub fn scene_sticker(size: u32) -> RasterImage {
    let s = size as f64;
    let c = s / 2.0;
    render(size, size, 8, move |x, y| {
        let d = dist(x, y, c, c);
        if d < 0.2 * s {
            WHITE
        } else if d < 0.4 * s {
            RED
        } else {
            CLEAR
        }
    })
}

/// A smooth freeform blob (neither circular nor straight) with a small inner highlight.
pub fn scene_blob(size: u32) -> RasterImage {
    let s = size as f64;
    let c = s / 2.0;
    render(size, size, 8, move |x, y| {
        let (dx, dy) = (x - c, y - c);
        let th = dy.atan2(dx);
        let radius = 0.36 * s * (1.0 + 0.22 * (3.0 * th).sin() + 0.09 * (5.0 * th + 1.0).cos());
        let d = dx.hypot(dy);
        if d < 0.45 * radius && dx < 0.0 {
            YELLOW
        } else if d < radius {
            GREEN
        } else {
            WHITE
        }
    })
}

/// A rounded rectangle badge (tangent line/arc joints without corners).
pub fn scene_rounded_rect(size: u32) -> RasterImage {
    let s = size as f64;
    let (x0, y0, x1, y1, r) = (0.15 * s, 0.25 * s, 0.85 * s, 0.75 * s, 0.14 * s);
    render(size, size, 8, move |x, y| {
        let qx = (x - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - r);
        let qy = (y - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - r);
        let inside = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) < r;
        if inside {
            BLUE
        } else {
            WHITE
        }
    })
}

/// Mimic a low-resolution / re-compressed web image: separable box blur of `radius` px plus
/// deterministic per-channel noise of +-`noise` levels (alpha untouched).
pub fn soften(img: &RasterImage, radius: usize, noise: i32) -> RasterImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut cur: Vec<f32> = img.data.iter().map(|&v| v as f32).collect();
    if radius > 0 {
        for horizontal in [true, false] {
            let src = cur.clone();
            for y in 0..h {
                for x in 0..w {
                    for c in 0..3 {
                        let (mut s, mut n) = (0.0f32, 0.0f32);
                        for d in -(radius as isize)..=radius as isize {
                            let (xx, yy) = if horizontal {
                                (x as isize + d, y as isize)
                            } else {
                                (x as isize, y as isize + d)
                            };
                            if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                                continue;
                            }
                            s += src[(yy as usize * w + xx as usize) * 4 + c];
                            n += 1.0;
                        }
                        cur[(y * w + x) * 4 + c] = s / n;
                    }
                }
            }
        }
    }
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut data = Vec::with_capacity(cur.len());
    for (i, v) in cur.iter().enumerate() {
        let mut o = *v;
        if i % 4 != 3 && noise > 0 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            o += ((s % (2 * noise as u64 + 1)) as i32 - noise) as f32;
        }
        data.push(o.round().clamp(0.0, 255.0) as u8);
    }
    RasterImage::new(img.width, img.height, data).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Reference rasteriser
// ---------------------------------------------------------------------------------------------

/// Flatten a contour; shared chains are flattened in canonical direction and reversed for
/// the twin side, so both neighbours see the identical polyline.
pub fn flatten_contour(doc: &VectorDocument, contour: &[ContourRef], tol: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for r in contour {
        let chain = &doc.chains[r.chain as usize];
        let mut pts: Vec<Point> = Vec::new();
        if let Some(first) = chain.prims.first() {
            pts.push(first.start());
        }
        for p in &chain.prims {
            p.flatten(tol, &mut pts);
        }
        if r.reversed {
            pts.reverse();
        }
        if !out.is_empty() {
            out.pop();
        }
        out.extend(pts);
    }
    out
}

/// Even-odd, painter-order rasterisation with `ss × ss` supersampling. Pixels not covered by
/// any shape are fully transparent. Also returns how many shapes covered each pixel's
/// samples (max over samples) via the `coverage_counts` output when requested.
pub fn rasterize(doc: &VectorDocument, ss: u32) -> RasterImage {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let ss = ss as usize;
    let (sw, sh) = (w * ss, h * ss);
    let mut color: Vec<[u8; 3]> = vec![[0; 3]; sw * sh];
    let mut covered = vec![false; sw * sh];
    paint(doc, ss, |idx, c, _| {
        color[idx] = c;
        covered[idx] = true;
    });
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let i = (y * ss + sy) * sw + x * ss + sx;
                    if covered[i] {
                        r += color[i][0] as u32;
                        g += color[i][1] as u32;
                        b += color[i][2] as u32;
                        a += 1;
                    }
                }
            }
            let n = (ss * ss) as u32;
            if a == 0 {
                data.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                data.extend_from_slice(&[
                    (r / a) as u8,
                    (g / a) as u8,
                    (b / a) as u8,
                    (a * 255 / n) as u8,
                ]);
            }
        }
    }
    RasterImage::new(doc.width, doc.height, data).unwrap()
}

/// For every sub-sample, how many shapes cover it (cutout topology must give exactly 1 inside
/// painted areas and 0 elsewhere – never 2 (overlap) nor a gap between touching shapes).
pub fn coverage_counts(doc: &VectorDocument, ss: u32) -> Vec<u8> {
    let ss = ss as usize;
    let mut counts = vec![0u8; doc.width as usize * ss * doc.height as usize * ss];
    paint(doc, ss, |idx, _, _| {
        counts[idx] = counts[idx].saturating_add(1)
    });
    counts
}

fn paint(doc: &VectorDocument, ss: usize, mut put: impl FnMut(usize, [u8; 3], usize)) {
    let sw = doc.width as usize * ss;
    let sh = doc.height as usize * ss;
    for (si, shape) in doc.shapes.iter().enumerate() {
        let polys: Vec<Vec<Point>> = shape
            .contours
            .iter()
            .map(|c| flatten_contour(doc, c, 0.01))
            .collect();
        for row in 0..sh {
            let y = (row as f64 + 0.5) / ss as f64;
            let mut xs: Vec<f64> = Vec::new();
            for poly in &polys {
                let n = poly.len();
                for i in 0..n {
                    let (a, b) = (poly[i], poly[(i + 1) % n]);
                    if (a.y <= y) != (b.y <= y) {
                        xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                    }
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in xs.chunks_exact(2) {
                let c0 = (pair[0] * ss as f64 - 0.5).ceil().max(0.0) as usize;
                let c1 = ((pair[1] * ss as f64 - 0.5).floor() + 1.0)
                    .min(sw as f64)
                    .max(0.0) as usize;
                for col in c0..c1 {
                    put(row * sw + col, shape.rgb, si);
                }
            }
        }
    }
}
