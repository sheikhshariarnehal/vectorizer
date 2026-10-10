//! `vectorizer-core`: a pure-Rust, deterministic raster → vector engine.
//!
//! Pipeline: Oklab conversion → bilateral denoise → K-Means palette → label map →
//! sub-pixel anti-aliasing inversion → watertight planar map → corner detection →
//! line / arc / circle / Bézier model selection → G¹ optimisation → SVG / DXF / PDF / EPS.
//!
//! ```no_run
//! use vectorizer_core::{vectorize, Params, RasterImage, SvgOptions};
//! let img = RasterImage::from_rgb(2, 2, &[255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255]).unwrap();
//! let doc = vectorize(&img, &Params::balanced()).unwrap();
//! let svg = doc.to_svg(&SvgOptions::default());
//! ```

pub mod color;
pub mod exporters;
pub mod fitting;
pub mod geom;
pub mod junction;
pub mod model;
pub mod optimizer;
pub mod pipeline;
pub mod primitives;
pub mod subpixel;
pub mod testing;
pub mod topology;
pub mod types;

pub use exporters::{DxfOptions, DxfUnits, SvgOptions};
pub use geom::Point;
pub use model::{Shape, Stats, VectorChain, VectorDocument};
pub use pipeline::vectorize;
pub use primitives::{Primitive, PrimitiveKind};
pub use types::{Params, RasterImage, TopologyMode, VectorizeError};
