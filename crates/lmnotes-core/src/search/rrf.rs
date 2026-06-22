//! Reciprocal Rank Fusion：融合多路检索结果（ADR-0003）。

/// 融合两路排名（rank 从 1 开始）。RRF 公式：score = Σ 1/(k + rank_i)。
pub fn fuse_scores(rank_a: usize, rank_b: usize, k: usize) -> f64 {
    1.0 / (k as f64 + rank_a as f64) + 1.0 / (k as f64 + rank_b as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_in_both_highest() {
        let both_top = fuse_scores(1, 1, 60);
        let only_a_top = fuse_scores(1, 100, 60);
        assert!(both_top > only_a_top, "出现在两路 top1 应高于仅一路 top1");
    }

    #[test]
    fn k_60_is_standard() {
        let s = fuse_scores(1, 2, 60);
        assert!(s > 0.0 && s < 0.05);
    }

    #[test]
    fn higher_rank_lower_score() {
        assert!(fuse_scores(1, 1, 60) > fuse_scores(5, 5, 60));
    }
}
