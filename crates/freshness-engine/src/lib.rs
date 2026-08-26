#![forbid(unsafe_code)]

#[must_use]
pub fn freshness_debt(age_ms: u16, motion: f32, residual: f32, edge: f32, confidence: u8) -> f32 {
    let age = f32::from(age_ms.min(1000)) / 1000.0;
    age * (1.0
        + 1.5 * motion.clamp(0.0, 1.0)
        + 0.75 * residual.clamp(0.0, 1.0)
        + 0.5 * edge.clamp(0.0, 1.0))
        * (1.0 + (1.0 - f32::from(confidence) / 255.0))
}
