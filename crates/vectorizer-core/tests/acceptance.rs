//! Acceptance matrix from the technical specification (section 6), on synthetic
//! anti-aliased scenes that are rendered with exact box-filter coverage.

use std::f64::consts::FRAC_PI_4;
use std::time::Instant;
use vectorizer_core::testing::*;
use vectorizer_core::*;

fn prims(doc: &VectorDocument) -> Vec<Primitive> {
    doc.chains
        .iter()
        .flat_map(|c| c.prims.iter().copied())
        .collect()
}

/// Mean absolute per-channel error (0..255) between two images composited on white.
fn mean_abs_error(a: &RasterImage, b: &RasterImage) -> f64 {
    let comp = |p: [u8; 4]| -> [f64; 3] {
        let al = p[3] as f64 / 255.0;
        [0, 1, 2].map(|i| p[i] as f64 * al + 255.0 * (1.0 - al))
    };
    let mut sum = 0.0;
    for y in 0..a.height {
        for x in 0..a.width {
            let (ca, cb) = (comp(a.pixel(x, y)), comp(b.pixel(x, y)));
            sum += (0..3).map(|i| (ca[i] - cb[i]).abs()).sum::<f64>() / 3.0;
        }
    }
    sum / (a.width * a.height) as f64
}

#[test]
fn geometry_circle_and_ring_become_native_circles() {
    let size = 200;
    let img = scene_circle_ring(size);
    let doc = vectorize(&img, &Params::balanced()).unwrap();
    let all = prims(&doc);
    let circles: Vec<_> = all
        .iter()
        .filter_map(|p| match p {
            Primitive::Circle { center, r, .. } => Some((*center, *r)),
            _ => None,
        })
        .collect();
    println!(
        "circle ring: {} circles, stats {:?}",
        circles.len(),
        doc.stats
    );
    assert_eq!(
        circles.len(),
        3,
        "disc + ring inner + ring outer; got {all:?}"
    );
    assert_eq!(doc.stats.beziers, 0, "zero wobbly Béziers");
    let s = size as f64;
    let mut radii: Vec<f64> = circles.iter().map(|c| c.1).collect();
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, want) in radii.iter().zip([0.28 * s, 0.38 * s, 0.45 * s]) {
        println!(
            "  radius {got:.4} (want {want:.4}, err {:.4})",
            (got - want).abs()
        );
        assert!(
            (got - want).abs() < 0.1,
            "radius error {} px",
            (got - want).abs()
        );
    }
    for (c, _) in &circles {
        assert!(
            c.dist(Point::new(s / 2.0, s / 2.0)) < 0.1,
            "center off by {}",
            c.dist(Point::new(s / 2.0, s / 2.0))
        );
    }
    let svg = doc.to_svg(&SvgOptions::default());
    assert!(svg.contains("<circle"), "native <circle> expected");
    assert!(svg.matches('A').count() >= 4, "ring is written with A arcs");
}

#[test]
fn lines_blueprint_is_all_straight_and_snapped() {
    let img = scene_blueprint(400);
    let doc = vectorize(&img, &Params::cad()).unwrap();
    let all = prims(&doc);
    println!("blueprint stats {:?}", doc.stats);
    assert_eq!(
        doc.stats.arcs + doc.stats.circles + doc.stats.beziers,
        0,
        "only L commands: {all:?}"
    );
    let mut long_lines = 0;
    for p in &all {
        if let Primitive::Line { a, b } = p {
            if a.dist(*b) > 10.0 {
                long_lines += 1;
                let ang = (b.y - a.y).atan2(b.x - a.x);
                let m = (ang / FRAC_PI_4).round();
                let dev = (ang - m * FRAC_PI_4).abs().to_degrees();
                assert!(
                    dev < 1e-6,
                    "line {a:?} -> {b:?} is {dev}° off a multiple of 45°"
                );
            }
        }
    }
    assert!(long_lines >= 8, "found {long_lines} long lines");
}

#[test]
fn topology_flag_has_no_seams_or_overlaps() {
    let img = scene_flag(240, 160);
    let doc = vectorize(&img, &Params::balanced()).unwrap();
    println!("flag stats {:?}", doc.stats);
    assert_eq!(doc.stats.palette_count, 5);
    let ss = 8u32;
    let counts = coverage_counts(&doc, ss);
    let total = counts.len() as f64;
    let gaps = counts.iter().filter(|&&c| c == 0).count() as f64;
    let overlaps = counts.iter().filter(|&&c| c > 1).count() as f64;
    println!("flag: gap samples {gaps}, overlap samples {overlaps} of {total}");
    assert!(gaps / total < 1e-5, "white hairline cracks: {gaps} samples");
    assert!(
        overlaps / total < 1e-5,
        "overlapping bleed: {overlaps} samples"
    );
    let err = mean_abs_error(&img, &rasterize(&doc, 4));
    println!("flag reconstruction MAE: {err:.3}");
    assert!(err < 3.0);
}

#[test]
fn alpha_sticker_has_clean_transparency() {
    let img = scene_sticker(200);
    let doc = vectorize(&img, &Params::balanced()).unwrap();
    println!("sticker stats {:?}", doc.stats);
    let out = rasterize(&doc, 8);
    // Corners must stay empty.
    for (x, y) in [(0, 0), (199, 0), (0, 199), (199, 199), (10, 10)] {
        assert_eq!(out.pixel(x, y)[3], 0, "pixel {x},{y} must be transparent");
    }
    // No dark / off-palette fringe: every reasonably opaque pixel is a mix of white and red.
    let red = [219.0f64, 31.0, 26.0];
    for y in 0..200 {
        for x in 0..200 {
            let p = out.pixel(x, y);
            if p[3] < 200 {
                continue;
            }
            let c = [p[0] as f64, p[1] as f64, p[2] as f64];
            // distance to the white–red segment
            let d = [255.0 - red[0], 255.0 - red[1], 255.0 - red[2]];
            let v = [c[0] - red[0], c[1] - red[1], c[2] - red[2]];
            let t = ((v[0] * d[0] + v[1] * d[1] + v[2] * d[2])
                / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]))
                .clamp(0.0, 1.0);
            let dist = (0..3)
                .map(|i| (v[i] - d[i] * t).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(dist < 12.0, "fringe colour {c:?} at {x},{y} (dist {dist})");
        }
    }
    let alpha_sum = |im: &RasterImage| -> f64 {
        (0..im.height)
            .flat_map(|y| (0..im.width).map(move |x| (x, y)))
            .map(|(x, y)| im.pixel(x, y)[3] as f64)
            .sum()
    };
    let (a0, a1) = (alpha_sum(&img), alpha_sum(&out));
    println!(
        "alpha mass original {a0}, vector {a1}, diff {:.3}%",
        (a1 - a0).abs() / a0 * 100.0
    );
    assert!((a1 - a0).abs() / a0 < 0.005);
    assert!(doc.shapes.iter().all(|s| s.rgb != [0, 0, 0]));
}

#[test]
fn rounded_rectangle_is_lines_and_arcs() {
    let doc = vectorize(&scene_rounded_rect(300), &Params::balanced()).unwrap();
    println!("rounded rect stats {:?}", doc.stats);
    assert!(
        doc.stats.arcs >= 4,
        "4 rounded corners as arcs: {:?}",
        doc.stats
    );
    assert_eq!(doc.stats.beziers, 0);
    assert!(doc.stats.lines >= 8, "4 sides (+ 4 border lines)");
}

#[test]
fn freeform_blob_uses_smooth_beziers_and_reconstructs_well() {
    let img = scene_blob(256);
    let doc = vectorize(&img, &Params::balanced()).unwrap();
    println!("blob stats {:?}", doc.stats);
    assert!(doc.stats.beziers >= 4);
    assert_eq!(doc.stats.circles, 0);
    let err = mean_abs_error(&img, &rasterize(&doc, 4));
    println!("blob reconstruction MAE {err:.3}");
    assert!(err < 1.5, "MAE {err}");
    // G1: tangents agree where consecutive primitives of a chain meet smoothly.
    let mut worst = 0.0f64;
    for chain in &doc.chains {
        let n = chain.prims.len();
        for i in 0..n {
            let j = (i + 1) % n;
            if j == i || (!chain.closed && j == 0) {
                continue;
            }
            let (a, b) = (chain.prims[i].end_tangent(), chain.prims[j].start_tangent());
            let ang = a.dot(b).clamp(-1.0, 1.0).acos().to_degrees();
            if ang < 10.0 {
                worst = worst.max(ang);
            }
        }
    }
    println!("largest residual tangent mismatch at smooth joints: {worst:.4} deg");
    assert!(worst < 0.5);
}

#[test]
fn exporters_produce_wellformed_output() {
    let doc = vectorize(&scene_rounded_rect(160), &Params::cad()).unwrap();
    let svg = doc.to_svg(&SvgOptions {
        primitive_overlay: true,
        ..Default::default()
    });
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(svg.contains("class=\"v-line\"") && svg.contains("class=\"v-arc\""));

    let dxf = doc.to_dxf(&DxfOptions {
        units: DxfUnits::Millimeters,
        scale: 0.2646,
    });
    let lines: Vec<&str> = dxf.lines().collect();
    assert_eq!(lines.len() % 2, 0, "group-code / value pairs");
    for pair in lines.chunks(2) {
        assert!(
            pair[0].trim().parse::<i32>().is_ok(),
            "bad group code {:?}",
            pair[0]
        );
    }
    assert!(dxf.contains("AC1015") && dxf.contains("\nARC\n") && dxf.contains("\nLINE\n"));
    assert!(dxf.contains("LAYER_01_#"));
    assert!(dxf.trim_end().ends_with("EOF"));

    let pdf = doc.to_pdf();
    assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
    // xref offsets (raw byte positions) must point at "N 0 obj".
    let tail_at = pdf.windows(10).rposition(|w| w == b"startxref\n").unwrap() + 10;
    let xref_pos: usize = std::str::from_utf8(&pdf[tail_at..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(pdf[xref_pos..].starts_with(b"xref"));
    let table = std::str::from_utf8(&pdf[xref_pos..]).unwrap();
    for (i, line) in table.lines().skip(3).take(4).enumerate() {
        let off: usize = line[..10].parse().unwrap();
        assert!(
            pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()),
            "xref entry {i}"
        );
    }

    let eps = doc.to_eps();
    assert!(
        eps.starts_with("%!PS-Adobe-3.0 EPSF-3.0") && eps.contains("%%BoundingBox: 0 0 160 160")
    );
    assert!(eps.contains(" arc") && eps.contains("lineto") && eps.contains("eofill"));
}

#[test]
fn stacked_mode_covers_everything_below_smaller_shapes() {
    let img = scene_circle_ring(160);
    let params = Params {
        topology: TopologyMode::Stacked,
        ..Params::balanced()
    };
    let doc = vectorize(&img, &params).unwrap();
    assert!(
        doc.shapes.iter().all(|s| s.contours.len() == 1),
        "no holes in stacked mode"
    );
    let areas: Vec<f64> = doc.shapes.iter().map(|s| s.area).collect();
    assert!(
        areas.windows(2).all(|w| w[0] >= w[1]),
        "big shapes first: {areas:?}"
    );
    let err = mean_abs_error(&img, &rasterize(&doc, 4));
    println!("stacked MAE {err:.3}");
    assert!(err < 3.0);
}

#[test]
fn performance_1024_square() {
    let img = scene_flag(1024, 1024);
    let params = Params::balanced();
    // warm-up (thread pool start-up)
    let _ = vectorize(&scene_flag(64, 64), &params).unwrap();
    let t = Instant::now();
    let doc = vectorize(&img, &params).unwrap();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("1024x1024 flag: {ms:.0} ms, stages {:?}", doc.stats.timings);
    assert!(ms < 3000.0, "far too slow: {ms} ms");
}

#[test]
fn noisy_input_still_recovers_primitives() {
    // +-8 levels of pseudo-random per-channel noise, roughly what JPEG artefacts look like.
    let mut img = scene_circle_ring(256);
    let mut s = 0x1234_5678_9abc_def0u64;
    for px in img.data.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let n = (s % 17) as i32 - 8;
            *c = (*c as i32 + n).clamp(0, 255) as u8;
        }
    }
    let doc = vectorize(&img, &Params::balanced()).unwrap();
    println!("noisy ring stats {:?}", doc.stats);
    assert_eq!(doc.stats.palette_count, 3);
    assert_eq!(doc.stats.shapes, 4, "no speckle shapes");
    assert_eq!(doc.stats.circles, 3, "circles survive noise");
    assert_eq!(doc.stats.beziers, 0);
}

/// Real-world inputs are blurred and noisy: edges must still come out as exact primitives
/// with no sliver shapes and a faithful reconstruction.
#[test]
fn soft_flag_has_clean_straight_edges_and_arcs() {
    for (radius, noise) in [(1usize, 3i32), (2, 5), (3, 6)] {
        let img = soften(&scene_flag(384, 256), radius, noise);
        let doc = vectorize(&img, &Params::balanced()).unwrap();
        println!("soft flag blur {radius} noise {noise}: {:?}", doc.stats);
        assert_eq!(
            doc.stats.palette_count, 5,
            "blend colours must not become palette entries"
        );
        assert_eq!(doc.stats.shapes, 6, "no sliver shapes (blur {radius})");
        assert_eq!(
            doc.stats.beziers, 0,
            "disc must be arcs, sides lines (blur {radius})"
        );
        let err = mean_abs_error(&img, &rasterize(&doc, 4));
        println!("  reconstruction MAE {err:.3}");
        assert!(err < 6.0 + radius as f64);
        // every long line is exactly axis-aligned or follows the triangle's two slanted edges
        for p in doc.chains.iter().flat_map(|c| c.prims.iter()) {
            if let Primitive::Line { a, b } = p {
                if a.dist(*b) > 40.0 {
                    let ang = (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(180.0);
                    let axis = [0.0, 90.0, 180.0].iter().any(|t| (ang - t).abs() < 1e-6);
                    let slanted = (ang - 41.9).abs() < 2.5 || (ang - 138.1).abs() < 2.5;
                    assert!(axis || slanted, "stray line angle {ang} (blur {radius})");
                }
            }
        }
    }
}

#[test]
fn soft_rounded_rect_and_circles_stay_geometric() {
    let rr = soften(&scene_rounded_rect(300), 2, 4);
    let doc = vectorize(&rr, &Params::balanced()).unwrap();
    println!("soft rounded rect {:?}", doc.stats);
    assert_eq!(doc.stats.shapes, 2);
    assert_eq!(doc.stats.beziers, 0);
    assert!(doc.stats.arcs >= 4 && doc.stats.lines >= 8);

    let ring = soften(&scene_circle_ring(256), 2, 4);
    let doc = vectorize(&ring, &Params::balanced()).unwrap();
    println!("soft ring {:?}", doc.stats);
    assert_eq!(
        (doc.stats.circles, doc.stats.beziers, doc.stats.shapes),
        (3, 0, 4)
    );
}

#[test]
fn deterministic_output() {
    let img = scene_flag(120, 80);
    let a = vectorize(&img, &Params::balanced())
        .unwrap()
        .to_svg(&SvgOptions::default());
    let b = vectorize(&img, &Params::balanced())
        .unwrap()
        .to_svg(&SvgOptions::default());
    assert_eq!(a, b);
}

#[test]
fn degenerate_inputs() {
    // single colour
    let flat = RasterImage::from_rgb(8, 8, &[200u8; 8 * 8 * 3]).unwrap();
    let doc = vectorize(&flat, &Params::balanced()).unwrap();
    assert_eq!(doc.shapes.len(), 1);
    // fully transparent
    let empty = RasterImage::new(8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    assert!(vectorize(&empty, &Params::balanced()).unwrap().is_empty());
    // 1x1
    let one = RasterImage::from_rgb(1, 1, &[1, 2, 3]).unwrap();
    assert_eq!(
        vectorize(&one, &Params::balanced()).unwrap().shapes.len(),
        1
    );
}
