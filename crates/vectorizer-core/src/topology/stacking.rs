//! Planar-cutout vs stacked-layer shape ordering.

use super::planar_graph::FaceTopo;
use crate::types::TopologyMode;

/// Order faces for painting and, in [`TopologyMode::Stacked`], close their holes so every
/// shape is a solid layer that gets covered by the smaller shapes drawn on top of it.
pub fn arrange(mut faces: Vec<FaceTopo>, mode: TopologyMode) -> Vec<FaceTopo> {
    if mode == TopologyMode::Stacked {
        for f in &mut faces {
            f.contours.truncate(1);
            f.areas.truncate(1);
        }
    }
    // Largest outer area first = painted first = lowest layer.
    faces.sort_by(|a, b| {
        let (aa, ba) = (
            a.areas.first().copied().unwrap_or(0.0),
            b.areas.first().copied().unwrap_or(0.0),
        );
        ba.partial_cmp(&aa).unwrap_or(std::cmp::Ordering::Equal)
    });
    faces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::planar_graph::ContourRef;

    fn face(label: u16, areas: Vec<f64>) -> FaceTopo {
        FaceTopo {
            label,
            contours: areas
                .iter()
                .map(|_| {
                    vec![ContourRef {
                        chain: 0,
                        reversed: false,
                    }]
                })
                .collect(),
            areas,
        }
    }

    #[test]
    fn stacked_drops_holes_and_sorts_big_first() {
        let out = arrange(
            vec![face(1, vec![10.0]), face(0, vec![100.0, 20.0])],
            TopologyMode::Stacked,
        );
        assert_eq!(out[0].label, 0);
        assert_eq!(out[0].contours.len(), 1);
    }

    #[test]
    fn cutout_keeps_holes() {
        let out = arrange(vec![face(0, vec![100.0, 20.0])], TopologyMode::Cutout);
        assert_eq!(out[0].contours.len(), 2);
    }
}
