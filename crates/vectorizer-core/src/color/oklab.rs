//! sRGB <-> Oklab (Björn Ottosson, 2020).

use super::Lab;
use crate::types::RasterImage;
use rayon::prelude::*;

fn srgb_lut() -> [f32; 256] {
    let mut lut = [0.0f32; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *v = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
    }
    lut
}

#[inline]
fn rgb_lin_to_oklab(r: f32, g: f32, b: f32) -> Lab {
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

pub fn srgb_to_oklab(r: u8, g: u8, b: u8) -> Lab {
    let lut = srgb_lut();
    rgb_lin_to_oklab(lut[r as usize], lut[g as usize], lut[b as usize])
}

/// Oklab → linear-light RGB (unclamped).
pub fn oklab_to_linear(lab: Lab) -> [f32; 3] {
    let l_ = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m_ = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
    let s_ = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// Oklab → gamma-encoded sRGB as floats in 0..=255 (no rounding, clamped).
pub fn oklab_to_srgb_f(lab: Lab) -> [f32; 3] {
    let l = oklab_to_linear(lab);
    [
        linear_to_srgb_f(l[0]) * 255.0,
        linear_to_srgb_f(l[1]) * 255.0,
        linear_to_srgb_f(l[2]) * 255.0,
    ]
}

/// Linear-light RGB → Oklab (negative components are clamped to zero).
pub fn linear_to_oklab(c: [f32; 3]) -> Lab {
    rgb_lin_to_oklab(c[0].max(0.0), c[1].max(0.0), c[2].max(0.0))
}

/// Linear-light component → gamma-encoded sRGB component (both in 0..1).
pub fn linear_to_srgb_f(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Gamma-encoded sRGB component → linear-light component (both in 0..1).
pub fn srgb_to_linear_f(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
fn encode_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

pub fn oklab_to_srgb(lab: Lab) -> [u8; 3] {
    let l_ = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m_ = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
    let s_ = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s;
    let b = -0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    [encode_srgb(r), encode_srgb(g), encode_srgb(b)]
}

/// Convert a whole raster (parallel). Transparent pixels still receive a value.
pub fn image_to_oklab(img: &RasterImage) -> Vec<Lab> {
    let lut = srgb_lut();
    img.data
        .par_chunks(4 * 4096)
        .flat_map_iter(|chunk| {
            chunk
                .chunks_exact(4)
                .map(|p| {
                    rgb_lin_to_oklab(lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize])
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black() {
        let w = srgb_to_oklab(255, 255, 255);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3);
        let k = srgb_to_oklab(0, 0, 0);
        assert!(k[0].abs() < 1e-6);
    }

    #[test]
    fn roundtrip() {
        for &(r, g, b) in &[
            (255u8, 0u8, 0u8),
            (12, 200, 99),
            (128, 128, 128),
            (3, 4, 250),
        ] {
            let back = oklab_to_srgb(srgb_to_oklab(r, g, b));
            assert!((back[0] as i32 - r as i32).abs() <= 1, "{back:?}");
            assert!((back[1] as i32 - g as i32).abs() <= 1, "{back:?}");
            assert!((back[2] as i32 - b as i32).abs() <= 1, "{back:?}");
        }
    }
}
