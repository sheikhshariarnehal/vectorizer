//! Connected-component analysis and speckle removal.

use super::LabelMap;

/// 4-connected components of equal labels.
pub struct Components {
    /// Component id of every pixel.
    pub id: Vec<u32>,
    /// Pixel count per component.
    pub area: Vec<u32>,
    /// Pixels of component `c` are `order[start[c]..start[c + 1]]`.
    pub order: Vec<u32>,
    pub start: Vec<u32>,
}

pub fn components(map: &LabelMap) -> Components {
    let (w, h) = (map.width, map.height);
    let mut id = vec![u32::MAX; w * h];
    let mut order: Vec<u32> = Vec::with_capacity(w * h);
    let mut start: Vec<u32> = Vec::new();
    let mut stack: Vec<u32> = Vec::new();
    for seed in 0..w * h {
        if id[seed] != u32::MAX {
            continue;
        }
        let cid = start.len() as u32;
        start.push(order.len() as u32);
        let label = map.labels[seed];
        id[seed] = cid;
        stack.push(seed as u32);
        while let Some(p) = stack.pop() {
            order.push(p);
            let p = p as usize;
            let (x, y) = (p % w, p / w);
            let mut visit = |q: usize| {
                if id[q] == u32::MAX && map.labels[q] == label {
                    id[q] = cid;
                    stack.push(q as u32);
                }
            };
            if x > 0 {
                visit(p - 1);
            }
            if x + 1 < w {
                visit(p + 1);
            }
            if y > 0 {
                visit(p - w);
            }
            if y + 1 < h {
                visit(p + w);
            }
        }
    }
    start.push(order.len() as u32);
    let area = start.windows(2).map(|s| s[1] - s[0]).collect();
    Components {
        id,
        area,
        order,
        start,
    }
}

/// Merge every component smaller than `min_area` into the neighbouring label it shares the
/// longest border with. Returns the number of components removed.
pub fn remove_speckles(map: &mut LabelMap, min_area: usize) -> usize {
    if min_area <= 1 {
        return 0;
    }
    let comps = components(map);
    let (w, h) = (map.width as isize, map.height as isize);
    let mut small: Vec<usize> = (0..comps.area.len())
        .filter(|&c| (comps.area[c] as usize) < min_area)
        .collect();
    small.sort_by_key(|&c| comps.area[c]);
    let mut removed = 0;
    let mut votes: Vec<(u16, u32)> = Vec::new();
    for c in small {
        let pixels = &comps.order[comps.start[c] as usize..comps.start[c + 1] as usize];
        let own = map.labels[pixels[0] as usize];
        votes.clear();
        for &p in pixels {
            let (x, y) = (
                (p as usize % map.width) as isize,
                (p as usize / map.width) as isize,
            );
            for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let l = map.labels[ny as usize * map.width + nx as usize];
                if l == own {
                    continue;
                }
                if let Some(v) = votes.iter_mut().find(|v| v.0 == l) {
                    v.1 += 1;
                } else {
                    votes.push((l, 1));
                }
            }
        }
        if let Some(best) = votes.iter().max_by_key(|v| v.1) {
            let l = best.0;
            for &p in pixels {
                map.labels[p as usize] = l;
            }
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_isolated_pixel() {
        let mut m = LabelMap::new(5, 5, vec![0; 25]);
        m.labels[12] = 1;
        assert_eq!(remove_speckles(&mut m, 4), 1);
        assert!(m.labels.iter().all(|&l| l == 0));
    }

    #[test]
    fn keeps_large_regions() {
        let mut m = LabelMap::new(4, 2, vec![0, 0, 1, 1, 0, 0, 1, 1]);
        assert_eq!(remove_speckles(&mut m, 4), 0);
        assert_eq!(components(&m).area.len(), 2);
    }
}
