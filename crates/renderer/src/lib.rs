#![forbid(unsafe_code)]

use std::time::{Duration, Instant};
use visual_state::{HEIGHT, REGION_HEIGHT, REGION_WIDTH, REGIONS_X, VisualState, WIDTH};

pub const FRAME_INTERVAL: Duration = Duration::from_nanos(33_333_333);
#[derive(Debug)]
pub struct FrameRenderer {
    rgba: Vec<u8>,
    next_tick: Instant,
    frames: u64,
}
impl FrameRenderer {
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self {
            rgba: vec![0; WIDTH * HEIGHT * 4],
            next_tick: now,
            frames: 0,
        }
    }
    #[must_use]
    pub fn is_due(&self, now: Instant) -> bool {
        now >= self.next_tick
    }
    pub fn render(&mut self, state: &VisualState, now: Instant) -> &[u8] {
        while self.next_tick <= now {
            self.next_tick += FRAME_INTERVAL;
        }
        for region in 0..2700_u16 {
            let Some(surface) = state.surface(region) else {
                continue;
            };
            let row = usize::from(region) / REGIONS_X;
            let col = usize::from(region) % REGIONS_X;
            if let Some(raw) = &surface.raw_rgb {
                for y in 0..REGION_HEIGHT {
                    for x in 0..REGION_WIDTH {
                        let source = (y * REGION_WIDTH + x) * 3;
                        let pixel =
                            ((row * REGION_HEIGHT + y) * WIDTH + col * REGION_WIDTH + x) * 4;
                        self.rgba[pixel..pixel + 3].copy_from_slice(&raw[source..source + 3]);
                        self.rgba[pixel + 3] = 255;
                    }
                }
                continue;
            }
            for y in 0..REGION_HEIGHT {
                for x in 0..REGION_WIDTH {
                    let gx = (x * 8 / REGION_WIDTH).min(7);
                    let gy = (y * 6 / REGION_HEIGHT).min(5);
                    let luma = i16::from(surface.luma[gy * 8 + gx]);
                    let ca = i16::from(
                        surface.chroma_a
                            [(y * 3 / REGION_HEIGHT).min(2) * 4 + (x * 4 / REGION_WIDTH).min(3)],
                    );
                    let cb = i16::from(
                        surface.chroma_b
                            [(y * 3 / REGION_HEIGHT).min(2) * 4 + (x * 4 / REGION_WIDTH).min(3)],
                    );
                    let g = luma - (ca + cb) / 4;
                    let pixel = ((row * REGION_HEIGHT + y) * WIDTH + col * REGION_WIDTH + x) * 4;
                    self.rgba[pixel] = (g + ca).clamp(0, 255) as u8;
                    self.rgba[pixel + 1] = g.clamp(0, 255) as u8;
                    self.rgba[pixel + 2] = (g + cb).clamp(0, 255) as u8;
                    self.rgba[pixel + 3] = 255;
                }
            }
        }
        self.frames += 1;
        &self.rgba
    }
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_is_fhd_rgba() {
        let mut r = FrameRenderer::new(Instant::now());
        assert_eq!(
            r.render(&VisualState::new(), Instant::now()).len(),
            WIDTH * HEIGHT * 4
        );
    }
}
