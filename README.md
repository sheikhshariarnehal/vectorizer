# Vectorizer – classical geometric raster → vector engine

Phase 1 + 2 of `vectorizer_engine_plan.md` / `vectorizer_technical_specification.md`:
the pure-Rust engine (`crates/vectorizer-core`), all four exporters, and a CLI
(`crates/vectorizer-cli`). The Tauri shell and studio UI (Phase 3) are not started.

## Pipeline

```
RGBA → Oklab → bilateral denoise → K-Means palette (blend-colour pruning)
     → label map → speckle removal
     → watertight planar map on the pixel-corner lattice (shared half-edge chains)
     → sub-pixel boundaries by anti-aliasing inversion (fractional marching squares)
     → corner detection → line / Pratt arc / circle / Schneider Bézier (MDL selection)
     → G¹ tangent optimisation → SVG · DXF (AC1015) · PDF · EPS
```

Module map follows the spec (`color/`, `subpixel/`, `topology/`, `primitives/`,
`optimizer/`, `exporters/`); `fitting.rs` orchestrates per-chain fitting, `pipeline.rs`
wires the stages, `testing.rs` has the synthetic scenes and a reference rasteriser.

## Use

```
cargo run --release -p vectorizer-cli -- convert logo.png -f svg,dxf,pdf,eps -p cad -o out
cargo run --release -p vectorizer-cli -- convert ./folder -o out          # batch, parallel
cargo run --release -p vectorizer-cli -- demo demo-out                    # synthetic scenes
cargo run --release -p vectorizer-cli -- bench logo.png --runs 10
cargo test -p vectorizer-core
```

```rust
let doc = vectorizer_core::vectorize(&img, &Params::balanced())?;
let svg = doc.to_svg(&SvgOptions::default());
let dxf = doc.to_dxf(&DxfOptions { units: DxfUnits::Millimeters, scale: 0.2646 });
```

## Notes

* `Params::{cad, balanced, artistic}` mirror the three UI presets; every slider maps to a field.
* Planar *cutout* is the default (zero gaps, zero overlap); `TopologyMode::Stacked` closes holes
  and orders shapes large → small for vinyl / laser work.
* Output is deterministic (fixed-chunk parallel reductions, fixed RNG seed).
* Sub-pixel inversion is done in gamma-encoded sRGB (where rasterisers blend), clustering in Oklab.
