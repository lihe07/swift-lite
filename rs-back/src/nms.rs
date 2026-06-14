/// A box as [x1, y1, x2, y2, score].
pub type Box5 = [f32; 5];

/// Non-maximum suppression. Port of back/common.py:nms.
/// Filters score > threshold (strict), then suppresses boxes with IoU > iou.
pub fn nms(boxes: &[Box5], threshold: f32, iou: f32) -> Vec<Box5> {
    // Filter by score > threshold (strict, matching numpy boolean mask).
    let mut idx: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i][4] > threshold)
        .collect();

    // Sort indices by score descending.
    idx.sort_by(|&a, &b| {
        boxes[b][4]
            .partial_cmp(&boxes[a][4])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let area = |b: &Box5| (b[3] - b[1]) * (b[2] - b[0]);

    let mut keep = Vec::new();
    while !idx.is_empty() {
        let i = idx[0];
        keep.push(boxes[i]);
        let bi = boxes[i];
        let ai = area(&bi);
        idx = idx[1..]
            .iter()
            .copied()
            .filter(|&j| {
                let bj = &boxes[j];
                let xx1 = bi[0].max(bj[0]);
                let yy1 = bi[1].max(bj[1]);
                let xx2 = bi[2].min(bj[2]);
                let yy2 = bi[3].min(bj[3]);
                let w = (xx2 - xx1).max(0.0);
                let h = (yy2 - yy1).max(0.0);
                let inter = w * h;
                let ovr = inter / (ai + area(bj) - inter);
                // numpy keeps where ovr <= iou
                ovr <= iou
            })
            .collect();
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_below_threshold() {
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [0.0, 0.0, 10.0, 10.0, 0.2],
        ];
        // second box has score 0.2 which is not > 0.3
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][4], 0.9);
    }

    #[test]
    fn suppresses_high_overlap() {
        // two near-identical boxes -> IoU ~1.0 > 0.5 -> keep highest score only
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [0.0, 0.0, 10.0, 10.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][4], 0.9);
    }

    #[test]
    fn keeps_disjoint_boxes() {
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [100.0, 100.0, 110.0, 110.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn keeps_partial_overlap_below_iou() {
        // overlap area 25, union 175 -> IoU ~0.143 <= 0.5 -> keep both
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [5.0, 5.0, 15.0, 15.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 2);
    }
}
