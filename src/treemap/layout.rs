/// Squarified treemap layout algorithm.
///
/// Based on: "Squarified Treemaps" by Bruls, Huizing, and van Wijk.

#[derive(Debug, Clone, Copy)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub node_index: usize,
}

/// Compute squarified treemap layout for items within a bounding rectangle.
///
/// `items` is a slice of (node_index, size) pairs, pre-sorted by size descending.
/// `bounds` is the available rectangle (x, y, w, h).
/// `padding` is the gap between rectangles.
pub fn squarify(items: &[(usize, u64)], bounds: (f32, f32, f32, f32), padding: f32) -> Vec<LayoutRect> {
    if items.is_empty() || bounds.2 <= 0.0 || bounds.3 <= 0.0 {
        return Vec::new();
    }

    let total_size: u64 = items.iter().map(|&(_, s)| s).sum();
    if total_size == 0 {
        return Vec::new();
    }

    let total_area = bounds.2 as f64 * bounds.3 as f64;
    // Convert sizes to areas proportional to the bounding rect
    let areas: Vec<f64> = items
        .iter()
        .map(|&(_, s)| (s as f64 / total_size as f64) * total_area)
        .collect();

    let mut result = Vec::with_capacity(items.len());
    let mut remaining = bounds;

    let mut i = 0;
    while i < items.len() {
        let (short_side, _long_side) = if remaining.2 <= remaining.3 {
            (remaining.2 as f64, remaining.3 as f64)
        } else {
            (remaining.3 as f64, remaining.2 as f64)
        };

        // Determine how many items to place in this row
        let mut row = vec![i];
        let mut row_area = areas[i];
        let mut best_worst = worst_ratio(&[areas[i]], short_side);

        let mut j = i + 1;
        while j < items.len() {
            let mut test_areas: Vec<f64> = row.iter().map(|&idx| areas[idx]).collect();
            test_areas.push(areas[j]);
            let test_worst = worst_ratio(&test_areas, short_side);
            if test_worst <= best_worst {
                best_worst = test_worst;
                row.push(j);
                row_area += areas[j];
                j += 1;
            } else {
                break;
            }
        }

        // Lay out the row
        let is_horizontal = remaining.2 <= remaining.3;
        if is_horizontal {
            // Row spans full width, variable height
            let row_h = if remaining.2 as f64 > 0.0 {
                row_area / remaining.2 as f64
            } else {
                0.0
            };

            let mut x = remaining.0;
            for &idx in &row {
                let item_w = if row_h > 0.0 {
                    (areas[idx] / row_h) as f32
                } else {
                    0.0
                };
                result.push(LayoutRect {
                    x: x + padding,
                    y: remaining.1 + padding,
                    w: (item_w - padding * 2.0).max(0.0),
                    h: (row_h as f32 - padding * 2.0).max(0.0),
                    node_index: items[idx].0,
                });
                x += item_w;
            }

            remaining.1 += row_h as f32;
            remaining.3 -= row_h as f32;
        } else {
            // Row spans full height, variable width
            let row_w = if remaining.3 as f64 > 0.0 {
                row_area / remaining.3 as f64
            } else {
                0.0
            };

            let mut y = remaining.1;
            for &idx in &row {
                let item_h = if row_w > 0.0 {
                    (areas[idx] / row_w) as f32
                } else {
                    0.0
                };
                result.push(LayoutRect {
                    x: remaining.0 + padding,
                    y: y + padding,
                    w: (row_w as f32 - padding * 2.0).max(0.0),
                    h: (item_h - padding * 2.0).max(0.0),
                    node_index: items[idx].0,
                });
                y += item_h;
            }

            remaining.0 += row_w as f32;
            remaining.2 -= row_w as f32;
        }

        i = j;
    }

    result
}

/// Compute the worst aspect ratio in a row of given areas laid out along `side`.
fn worst_ratio(areas: &[f64], side: f64) -> f64 {
    if areas.is_empty() || side <= 0.0 {
        return f64::MAX;
    }

    let sum: f64 = areas.iter().sum();
    let mut worst = 0.0f64;

    for &area in areas {
        if area <= 0.0 {
            continue;
        }
        // Standard formula: ratio = max(w/h, h/w) for each item
        // where one dimension is `sum/side` and the other is `area/(sum/side)`
        let r = (side * side * area) / (sum * sum);
        let aspect = if r > 1.0 { r } else { 1.0 / r };
        worst = worst.max(aspect);
    }

    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = squarify(&[], (0.0, 0.0, 100.0, 100.0), 0.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_item() {
        let result = squarify(&[(0, 100)], (0.0, 0.0, 200.0, 100.0), 0.0);
        assert_eq!(result.len(), 1);
        let r = &result[0];
        assert!((r.w - 200.0).abs() < 1.0);
        assert!((r.h - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_two_equal_items() {
        let items = vec![(0, 500), (1, 500)];
        let result = squarify(&items, (0.0, 0.0, 200.0, 100.0), 0.0);
        assert_eq!(result.len(), 2);

        // Total area should equal bounding rect area
        let total_area: f32 = result.iter().map(|r| r.w * r.h).sum();
        assert!((total_area - 20000.0).abs() < 10.0);
    }

    #[test]
    fn test_areas_proportional() {
        let items = vec![(0, 600), (1, 300), (2, 100)];
        let result = squarify(&items, (0.0, 0.0, 100.0, 100.0), 0.0);
        assert_eq!(result.len(), 3);

        let areas: Vec<f32> = result.iter().map(|r| r.w * r.h).collect();
        // First item should have ~60% of area
        assert!((areas[0] / 10000.0 - 0.6).abs() < 0.05);
    }

    #[test]
    fn test_no_overlap() {
        let items: Vec<(usize, u64)> = (0..10).map(|i| (i, (10 - i as u64) * 100)).collect();
        let result = squarify(&items, (0.0, 0.0, 400.0, 300.0), 0.0);

        // Check no two rectangles overlap
        for i in 0..result.len() {
            for j in (i + 1)..result.len() {
                let a = &result[i];
                let b = &result[j];
                let overlap_x = a.x < b.x + b.w && a.x + a.w > b.x;
                let overlap_y = a.y < b.y + b.h && a.y + a.h > b.y;
                assert!(
                    !(overlap_x && overlap_y),
                    "Rectangles {} and {} overlap",
                    i,
                    j
                );
            }
        }
    }

    #[test]
    fn test_with_padding() {
        let items = vec![(0, 500), (1, 500)];
        let result = squarify(&items, (0.0, 0.0, 200.0, 100.0), 2.0);
        assert_eq!(result.len(), 2);

        // All rects should be inside bounds
        for r in &result {
            assert!(r.x >= 0.0);
            assert!(r.y >= 0.0);
            assert!(r.x + r.w <= 200.0 + 0.1);
            assert!(r.y + r.h <= 100.0 + 0.1);
        }
    }
}
