//! Public input / configuration types.

use std::fmt;

/// Straight (non-premultiplied) RGBA8 raster, row-major, top-left origin.
#[derive(Clone, Debug)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes: R, G, B, A.
    pub data: Vec<u8>,
}

impl RasterImage {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, VectorizeError> {
        if width == 0 || height == 0 {
            return Err(VectorizeError::EmptyImage);
        }
        let expected = width as usize * height as usize * 4;
        if data.len() != expected {
            return Err(VectorizeError::BadBuffer {
                expected,
                got: data.len(),
            });
        }
        Ok(RasterImage {
            width,
            height,
            data,
        })
    }

    /// Build from tightly packed RGB8 (alpha is set to 255).
    pub fn from_rgb(width: u32, height: u32, rgb: &[u8]) -> Result<Self, VectorizeError> {
        let expected = width as usize * height as usize * 3;
        if rgb.len() != expected {
            return Err(VectorizeError::BadBuffer {
                expected,
                got: rgb.len(),
            });
        }
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for px in rgb.chunks_exact(3) {
            data.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
        Self::new(width, height, data)
    }

    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }
}

#[derive(Debug)]
pub enum VectorizeError {
    EmptyImage,
    BadBuffer { expected: usize, got: usize },
    ImageTooLarge { width: u32, height: u32 },
}

impl fmt::Display for VectorizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorizeError::EmptyImage => write!(f, "image has zero width or height"),
            VectorizeError::BadBuffer { expected, got } => {
                write!(f, "pixel buffer has {got} bytes, expected {expected}")
            }
            VectorizeError::ImageTooLarge { width, height } => {
                write!(
                    f,
                    "image {width}x{height} exceeds the supported size (max 16384 px per side)"
                )
            }
        }
    }
}

impl std::error::Error for VectorizeError {}

/// How colour regions are assembled into output shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyMode {
    /// Planar map: every boundary exists once, regions never overlap and never leave gaps.
    Cutout,
    /// Layered: each region is drawn filled (holes closed), larger shapes underneath.
    Stacked,
}

/// Tunable engine parameters (mirrors the studio UI sliders).
#[derive(Clone, Debug)]
pub struct Params {
    /// Number of palette colours `K` (2..=64).
    pub palette_colors: usize,
    /// Line fitting tolerance in px (0.1..=3.0).
    pub line_tolerance: f64,
    /// Arc fitting tolerance in px (0.1..=2.0).
    pub arc_tolerance: f64,
    /// Cubic Bézier fitting tolerance in px.
    pub curve_tolerance: f64,
    /// Interior angle (degrees, 90..=160) below which a vertex is a corner.
    pub corner_angle: f64,
    /// Lines within this many degrees of 0/45/90/135 are snapped. `0` disables snapping.
    pub snap_angle: f64,
    /// Largest distance (px) a vertex may move when a line is snapped to an exact angle.
    pub snap_max_shift: f64,
    pub topology: TopologyMode,
    /// Bilateral filter radius in px (0 disables the filter).
    pub denoise_radius: usize,
    /// Connected regions smaller than this many pixels are merged into their neighbour.
    pub speckle_area: usize,
    /// Lloyd iterations for K-Means.
    pub kmeans_iterations: usize,
    /// Pixels with alpha below this value are treated as transparent.
    pub alpha_threshold: u8,
    /// Drop palette entries that are merely anti-aliasing blends of two other entries.
    pub prune_blend_colors: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params::balanced()
    }
}

impl Params {
    /// Tight tolerances, aggressive orthogonal snapping – for technical drawings.
    pub fn cad() -> Self {
        Params {
            palette_colors: 8,
            line_tolerance: 0.3,
            arc_tolerance: 0.4,
            curve_tolerance: 0.4,
            corner_angle: 140.0,
            snap_angle: 3.0,
            snap_max_shift: 2.0,
            topology: TopologyMode::Cutout,
            denoise_radius: 0,
            speckle_area: 3,
            kmeans_iterations: 12,
            alpha_threshold: 128,
            prune_blend_colors: true,
        }
    }

    pub fn balanced() -> Self {
        Params {
            palette_colors: 16,
            line_tolerance: 0.8,
            arc_tolerance: 0.7,
            curve_tolerance: 0.8,
            corner_angle: 135.0,
            snap_angle: 2.0,
            snap_max_shift: 0.6,
            topology: TopologyMode::Cutout,
            denoise_radius: 1,
            speckle_area: 4,
            kmeans_iterations: 10,
            alpha_threshold: 128,
            prune_blend_colors: true,
        }
    }

    pub fn artistic() -> Self {
        Params {
            palette_colors: 24,
            line_tolerance: 1.5,
            arc_tolerance: 1.0,
            curve_tolerance: 1.5,
            corner_angle: 120.0,
            snap_angle: 1.0,
            snap_max_shift: 0.6,
            topology: TopologyMode::Cutout,
            denoise_radius: 2,
            speckle_area: 12,
            kmeans_iterations: 8,
            alpha_threshold: 128,
            prune_blend_colors: true,
        }
    }

    /// Clamp every field into its documented range.
    pub fn sanitized(&self) -> Params {
        let mut p = self.clone();
        p.palette_colors = p.palette_colors.clamp(2, 64);
        p.line_tolerance = p.line_tolerance.clamp(0.05, 5.0);
        p.arc_tolerance = p.arc_tolerance.clamp(0.05, 5.0);
        p.curve_tolerance = p.curve_tolerance.clamp(0.05, 5.0);
        p.corner_angle = p.corner_angle.clamp(60.0, 170.0);
        p.snap_angle = p.snap_angle.clamp(0.0, 5.0);
        p.snap_max_shift = p.snap_max_shift.clamp(0.0, 4.0);
        p.denoise_radius = p.denoise_radius.min(3);
        p.kmeans_iterations = p.kmeans_iterations.clamp(1, 50);
        p
    }
}
