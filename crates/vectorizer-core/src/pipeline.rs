//! End-to-end raster → vector pipeline.

use crate::color::{bilateral::bilateral, blend, kmeans, oklab};
use crate::fitting::fit_all;
use crate::model::{Shape, StageTimings, Stats, VectorChain, VectorDocument};
use crate::primitives::PrimitiveKind;
use crate::subpixel::{alpha_matte, CoverageField};
use crate::topology::{planar_graph, speckle, stacking, LabelMap, TRANSPARENT};
use crate::types::{Params, RasterImage, VectorizeError};
use std::time::Instant;

const MAX_SIDE: u32 = 16_384;
/// Oklab distance beyond which a pixel is treated as a blend rather than a palette colour.
const BLEND_TAU: f32 = 0.05;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Vectorize `img` with `params`.
pub fn vectorize(img: &RasterImage, params: &Params) -> Result<VectorDocument, VectorizeError> {
    if img.width > MAX_SIDE || img.height > MAX_SIDE {
        return Err(VectorizeError::ImageTooLarge {
            width: img.width,
            height: img.height,
        });
    }
    let params = params.sanitized();
    let (w, h) = (img.width as usize, img.height as usize);
    let t_total = Instant::now();
    let mut timings = StageTimings::default();

    // ---- colour preparation -------------------------------------------------------------------
    let t = Instant::now();
    let opaque = alpha_matte::opaque_mask(img, params.alpha_threshold);
    let solid = alpha_matte::solid_mask(img);
    let lab = oklab::image_to_oklab(img);
    let filtered = bilateral(&lab, &opaque, w, h, params.denoise_radius);
    drop(lab);
    timings.prepare_ms = ms(t);

    // ---- palette & label map ------------------------------------------------------------------
    let t = Instant::now();
    let train: &[bool] = if solid.iter().any(|&s| s) {
        &solid
    } else {
        &opaque
    };
    let (centroids, counts) = kmeans::quantize(
        &filtered,
        train,
        params.palette_colors,
        params.kmeans_iterations,
    );
    let palette_lab = kmeans::refine_palette(centroids, counts, params.prune_blend_colors);
    if palette_lab.is_empty() {
        // Fully transparent image.
        return Ok(empty_document(img, &params, timings, t_total));
    }
    let mut labels = kmeans::assign(&filtered, &opaque, &palette_lab, TRANSPARENT);
    blend::resolve_blends(
        &filtered,
        &opaque,
        &mut labels,
        &palette_lab,
        w,
        h,
        BLEND_TAU,
    );

    drop(filtered);
    alpha_matte::heal_fringe(&mut labels, img, &solid, &opaque);
    let mut map = LabelMap::new(w, h, labels);
    speckle::remove_speckles(&mut map, params.speckle_area);
    let comps = speckle::components(&map);
    timings.quantize_ms = ms(t);

    // ---- planar topology with sub-pixel boundaries ------------------------------------------
    let t = Instant::now();
    let palette_srgb: Vec<[f32; 3]> = palette_lab
        .iter()
        .map(|&c| oklab::oklab_to_srgb_f(c))
        .collect();
    let field = CoverageField {
        width: w,
        height: h,
        rgba: &img.data,
        palette: &palette_srgb,
    };
    let mut planar = planar_graph::build(&map, &field, &comps);
    let faces = stacking::arrange(std::mem::take(&mut planar.faces), params.topology);
    let mut used = vec![false; planar.chains.len()];
    for f in &faces {
        for c in &f.contours {
            for r in c {
                used[r.chain as usize] = true;
            }
        }
    }
    timings.topology_ms = ms(t);

    // ---- primitive fitting --------------------------------------------------------------------
    let t = Instant::now();
    let fitted = fit_all(&mut planar.chains, &mut planar.nodes, &used, &params, w, h);
    timings.fit_ms = ms(t);

    // ---- assemble document --------------------------------------------------------------------
    let palette: Vec<[u8; 3]> = palette_lab
        .iter()
        .map(|&c| oklab::oklab_to_srgb(c))
        .collect();
    let mut corners = Vec::new();
    let mut chains = Vec::with_capacity(planar.chains.len());
    for (ch, out) in planar.chains.iter().zip(fitted) {
        corners.extend(out.corners);
        chains.push(VectorChain {
            left: ch.left,
            right: ch.right,
            closed: ch.closed,
            prims: out.prims,
        });
    }
    let shapes: Vec<Shape> = faces
        .into_iter()
        .map(|f| Shape {
            palette_index: f.label as usize,
            rgb: palette[f.label as usize],
            area: f.areas.first().copied().unwrap_or(0.0),
            contours: f.contours,
        })
        .collect();

    let mut doc = VectorDocument {
        width: img.width,
        height: img.height,
        topology: params.topology,
        palette,
        chains,
        shapes,
        junctions: planar.nodes,
        corners,
        stats: Stats::default(),
    };
    timings.total_ms = ms(t_total);
    doc.stats = Stats {
        width: img.width,
        height: img.height,
        palette_count: doc.palette.len(),
        shapes: doc.shapes.len(),
        chains: doc.chains.iter().filter(|c| !c.prims.is_empty()).count(),
        lines: doc.count(PrimitiveKind::Line),
        arcs: doc.count(PrimitiveKind::Arc),
        circles: doc.count(PrimitiveKind::Circle),
        beziers: doc.count(PrimitiveKind::Cubic),
        nodes: doc.junctions.len() + doc.corners.len(),
        timings,
    };
    Ok(doc)
}

fn empty_document(
    img: &RasterImage,
    params: &Params,
    mut timings: StageTimings,
    t0: Instant,
) -> VectorDocument {
    timings.total_ms = ms(t0);
    VectorDocument {
        width: img.width,
        height: img.height,
        topology: params.topology,
        palette: Vec::new(),
        chains: Vec::new(),
        shapes: Vec::new(),
        junctions: Vec::new(),
        corners: Vec::new(),
        stats: Stats {
            width: img.width,
            height: img.height,
            timings,
            ..Stats::default()
        },
    }
}
