//! Anti-aliasing gradient inversion: recover the fractional coverage α of a boundary pixel.

/// α = ⟨I − C₂, C₁ − C₂⟩ / ‖C₁ − C₂‖², clamped to `[0, 1]`: the share of colour `c1` in the
/// pixel `i`, assuming `i` is a linear blend of `c1` and `c2` (in whatever linear colour
/// space the three vectors are expressed).
#[inline]
pub fn blend_alpha(i: &[f32; 3], c1: &[f32; 3], c2: &[f32; 3]) -> f32 {
    let d = [c1[0] - c2[0], c1[1] - c2[1], c1[2] - c2[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 < 1e-6 {
        return 0.5;
    }
    let v = [i[0] - c2[0], i[1] - c2[1], i[2] - c2[2]];
    ((v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len2).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_mix_ratio() {
        let c1 = [230.0f32, 20.0, 10.0];
        let c2 = [10.0f32, 90.0, 200.0];
        for &a in &[0.0f32, 0.25, 0.5, 0.8, 1.0] {
            let i = [
                c1[0] * a + c2[0] * (1.0 - a),
                c1[1] * a + c2[1] * (1.0 - a),
                c1[2] * a + c2[2] * (1.0 - a),
            ];
            assert!((blend_alpha(&i, &c1, &c2) - a).abs() < 1e-4);
        }
    }

    #[test]
    fn identical_colours_are_ambiguous() {
        let c = [10.0f32, 10.0, 10.0];
        assert_eq!(blend_alpha(&c, &c, &c), 0.5);
    }
}
