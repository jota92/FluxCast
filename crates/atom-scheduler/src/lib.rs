#![forbid(unsafe_code)]

#[must_use]
pub fn priority(
    visual_gain: f32,
    freshness_debt: f32,
    arrival_probability: f32,
    bytes: usize,
) -> f32 {
    if bytes == 0 {
        return 0.0;
    }
    visual_gain.max(0.0) * (1.0 + freshness_debt.max(0.0)) * arrival_probability.clamp(0.0, 1.0)
        / bytes as f32
}
