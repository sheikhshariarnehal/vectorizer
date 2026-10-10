# Vectorizer Desktop: Rust Architecture & Implementation Plan
*Lightweight Windows Desktop Vectorization Engine (100% Vectorizer.ai Algorithmic Match)*

---

## 1. Executive Summary & Core Decisions

This project is a high-performance, lightweight Windows desktop application built with **Rust** and **Tauri v2** that replicates the exact mathematical pipeline of **Vectorizer.ai** / **Vector Magic**.

### Key Architectural Decisions (From Grill-Me Session)
- **App Shell**: **Tauri v2** (Rust backend + native Windows WebView2, <15MB binary, instant launch, ultra-low RAM footprint).
- **Engine Core**: **Pure Classical Mathematical Engine in Rust** (multi-threaded with `rayon`). Uses James Diebel’s Stanford AI Lab foundation: *Bayesian Inversion of Anti-Aliased Rasterization*. Zero heavy neural network dependencies, 100% local, deterministic, and blazing fast.
- **Topology**: **Planar Map (Shared Half-Edge Graph)** for 0 white cracks and 0 overlapping bleeds, with a user-configurable **Stacked Layers** toggle (for vinyl plotting, CNC, and laser cutting).
- **Multi-Primitive Fitting**:
  - Straight Lines (`L`) with orthogonal snapping ($0^\circ, 90^\circ, 45^\circ$).
  - Native Circular Arcs (`A`) and Full Circles via **Pratt / Kåsa Algebraic Circle Fitting**.
  - Adaptive Cubic Bézier Splines (`C`) with $G^1$ tangency continuity at junctions.
- **Export Formats**: **SVG**, **DXF** (CAD/CNC/Laser), **PDF**, and **EPS**.
- **Studio Interface**:
  - Synchronized side-by-side split-screen zoom & pan canvas (Original Raster vs Vector).
  - **Interactive Primitive Inspector** (visual color-coding: 🔵 Lines, 🟢 Arcs, 🟠 Béziers, 🔴 Junctions).
  - Live parameter tuning (Palette size, arc sensitivity, curve tolerance, corner sharpness).
  - **Batch Processing Mode** (multi-file drag-and-drop queue).

---

## 2. System Architecture & Data Flow

```mermaid
flowchart TD
    subgraph Frontend [Tauri v2 WebView2 Frontend]
        UI[Side-by-Side Zoom/Pan Studio]
        Insp[Interactive Primitive Inspector]
        Batch[Batch Processing Queue]
    end

    subgraph Rust_IPC [Tauri IPC Bridge]
        Commands[Rust Tauri Commands: vectorize_image, batch_export]
    end

    subgraph Rust_Core [vectorizer-core (Pure Rust + Rayon)]
        direction TB
        A[Input Bitmap Buffer] --> B[Oklab Perceptual Color Space Conversion]
        B --> C[Bilateral Edge-Preserving Denoising]
        C --> D[Multi-Core K-Means Palette Quantization]
        D --> E[Sub-Pixel Anti-Aliasing Inversion: Continuous Marching Squares]
        E --> F[Planar Topology: Shared Boundary Graph]
        F --> G[Corner & Discontinuity Detection]
        G --> H{Multi-Primitive Classifier & Fitter}
        H -->|Linear Residual < Tol| I[Straight Line L]
        H -->|Radial Residual < Tol| J[Pratt Circular Arc A / Circle]
        H -->|Freeform Curve| K[Schneider Cubic Bézier C]
        I --> L[Global G1 Tangency & Angle Snapping Optimizer]
        J --> L
        K --> L
    end

    subgraph Exporters [Format Exporters]
        L --> EXP_SVG[Clean SVG Exporter]
        L --> EXP_DXF[AutoCAD DXF Exporter]
        L --> EXP_PDF[PDF Vector Exporter]
        L --> EXP_EPS[EPS PostScript Exporter]
    end

    Frontend <--> Rust_IPC <--> Rust_Core
    Rust_Core --> Exporters --> Frontend
```

---

## 3. Project Directory Structure

```text
d:\Poject\vectorizer\
├── Cargo.toml                    # Workspace configuration
├── crates/
│   ├── vectorizer-core/          # Pure computational geometry engine (lib crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── color/            # Oklab color space & palette quantization
│   │       │   ├── mod.rs
│   │       │   ├── oklab.rs
│   │       │   └── kmeans.rs
│   │       ├── subpixel/         # Inversion of anti-aliasing & sub-pixel marching squares
│   │       │   ├── mod.rs
│   │       │   ├── distance_field.rs
│   │       │   └── marching_squares.rs
│   │       ├── topology/         # Planar map & shared half-edge graph
│   │       │   ├── mod.rs
│   │       │   ├── planar_graph.rs
│   │       │   └── stacking.rs
│   │       ├── primitives/       # Multi-primitive fitting engine
│   │       │   ├── mod.rs
│   │       │   ├── line.rs
│   │       │   ├── circle_arc.rs # Pratt / Kåsa algebraic circle fitting
│   │       │   ├── bezier.rs     # Schneider adaptive cubic Bézier fitting
│   │       │   └── classifier.rs # Minimum Description Length (MDL) model selection
│   │       ├── optimizer/        # Tangency G1 continuity & corner angle snapping
│   │       │   ├── mod.rs
│   │       │   └── tangency.rs
│   │       └── exporters/        # Vector format writers
│   │           ├── mod.rs
│   │           ├── svg.rs
│   │           ├── dxf.rs
│   │           ├── pdf.rs
│   │           └── eps.rs
│   └── vectorizer-cli/           # High-speed CLI binary for testing & batch runs
│       ├── Cargo.toml
│       └── src/main.rs
├── src-tauri/                    # Tauri v2 Windows desktop application backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       └── commands.rs
└── ui/                           # High-performance desktop studio UI
    ├── index.html
    ├── style.css
    ├── app.js
    ├── canvas_viewer.js          # Synchronized split-screen zoom/pan canvas
    └── primitive_inspector.js    # Color-coded primitive visualizer
```

---

## 4. Algorithmic Breakdown of `vectorizer-core`

### 4.1. Perceptual Color Quantization (`color/`)
- Converts sRGB buffer to **Oklab** space $(L, a, b)$ where perceptual color distances match human vision:
  $$\Delta E_{\text{ok}} = \sqrt{(L_1 - L_2)^2 + (a_1 - a_2)^2 + (b_1 - b_2)^2}$$
- Applies bilateral edge-preserving filter in Oklab to suppress JPEG compression noise while preserving microscopic 1-pixel edges.
- Multi-threaded K-Means clustering (`rayon`) to partition the image into $K$ dominant palette colors.

### 4.2. Sub-Pixel Anti-Aliasing Inversion (`subpixel/`)
- Inverts the box-filter anti-aliasing:
  $$\alpha(x, y) = \frac{\|I(x, y) - C_{\text{bg}}\|}{\|C_{\text{fg}} - C_{\text{bg}}\|}$$
- Computes continuous sub-pixel coordinates $(x, y) \in \mathbb{R}^2$ via **Marching Squares with linear fractional interpolation** along grid cell boundaries:
  $$p_{\text{subpixel}} = p_0 + \frac{0.5 - v_0}{v_1 - v_0}(p_1 - p_0)$$
- Yields smooth, sub-pixel polylines with zero pixel-stair stepping.

### 4.3. Multi-Primitive Classifier & Fitter (`primitives/`)
Between detected corner vertices:
1. **Collinear Line Fit (`line.rs`)**:
   - Orthogonal distance error via Total Least Squares.
   - Snaps lines within $\pm 2^\circ$ of $0^\circ, 45^\circ, 90^\circ$.
   - Output: SVG `L x y`.
2. **Algebraic Circular Arc Fit (`circle_arc.rs`)**:
   - **Pratt's Algebraic Circle Fitting** algorithm: finds center $(x_c, y_c)$ and radius $R$ minimizing algebraic distance subject to constraint $B^2 - 4AC = 1$.
   - Verifies radius variance $\sigma_R < \epsilon_{\text{arc}}$.
   - Detects full circles ($\Delta \theta \approx 2\pi$) vs circular arcs ($\Delta \theta < 2\pi$).
   - Output: SVG `A rx ry 0 large_arc sweep x y` or `<circle>`.
3. **Adaptive Cubic Bézier Fit (`bezier.rs`)**:
   - Schneider's algorithm: least-squares parameterization with chord-length or centripetal knots.
   - If maximum error exceeds tolerance, recursively splits at peak residual.
   - Output: SVG `C x1 y1 x2 y2 x y`.

### 4.4. Planar Graph & Stacking (`topology/`)
- **Cutout Planar Map**: Every boundary separating color region $A$ and $B$ is represented as a single shared half-edge. Faces are formed by traversing these edges. Rendering adjacent faces creates 0 white gaps and 0 overlapping bleeds.
- **Stacked Layers**: Sorts shapes by area (outer bounding shapes rendered first, smaller foreground shapes rendered on top), ideal for vinyl plotting and laser engraving.

### 4.5. Format Exporters (`exporters/`)
- **SVG**: Clean, optimized, indent-free or formatted, semantic elements, CSS classes for styling.
- **DXF**: Standard AC1015 / AutoCAD format containing `LINE`, `ARC`, `LWPOLYLINE`, and `SPLINE` entities organized by layer color.
- **PDF & EPS**: Standard PostScript operators (`moveto`, `lineto`, `curveto`, `arc`, `fill`).

---

## 5. Development Phases & Implementation Sequence

### Phase 1: High-Performance Rust Core (`crates/vectorizer-core`)
- Scaffold Cargo workspace and crates.
- Implement Oklab color conversions and fast K-Means.
- Implement sub-pixel marching squares and corner detector.
- Implement Pratt circle/arc fitter, line fitter, and Schneider Bézier fitter.
- Implement planar map topology and SVG writer.
- Add CLI test runner (`crates/vectorizer-cli`) with test image benchmarks.

### Phase 2: Multi-Format Exporters
- Add DXF exporter (laser/CNC/vinyl plotters).
- Add PDF and EPS vector exporters.
- Validate DXF in CAD parsers and SVG in Chrome/Inkscape.

### Phase 3: Tauri v2 Desktop Studio UI
- Scaffold Tauri v2 Windows app (`src-tauri` and `ui/`).
- Build side-by-side synchronized zoom/pan canvas (Original vs Vectorized).
- Implement interactive Primitive Inspector (toggle color-coded lines, arcs, beziers, and nodes).
- Build batch processing queue and export dialog.
- Build release executable and verify file size (< 15MB).

---

## 6. Verification Milestones

| Milestone | Target | Acceptance Criteria |
| :--- | :--- | :--- |
| **VM1** | Primitive Verification | Circle test image outputs exact `<circle>` or `A` arc commands; geometric badges have straight `L` lines. |
| **VM2** | Seam & Bleed Test | Multi-colored graphic zoomed to 3200% displays 0 white hairline cracks between shapes. |
| **VM3** | CAD / DXF Test | Generated DXF opens cleanly in AutoCAD/LibreCAD with native circles and lines. |
| **VM4** | Performance & Size | Desktop installer < 15MB; vectorization of 1024x1024 image takes < 350ms on CPU. |
