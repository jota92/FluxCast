#![forbid(unsafe_code)]

use fcst_protocol::{Header, REGION_COUNT, Surface};
use std::time::{Duration, Instant};

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const REGION_WIDTH: usize = 32;
pub const REGION_HEIGHT: usize = 24;
pub const REGIONS_X: usize = 60;

#[derive(Debug, Clone)]
pub struct RegionState {
    pub state_id: u32,
    pub surface: Surface,
    pub updated_at: Instant,
    pub confidence: u8,
}
#[derive(Debug)]
pub struct VisualState {
    regions: Vec<Option<RegionState>>,
    applied_atoms: u64,
    rejected_base: u64,
}
#[derive(Debug, Clone, Copy)]
pub struct StateMetrics {
    pub applied_atoms: u64,
    pub rejected_base: u64,
    pub mean_age_ms: u64,
    pub p95_age_ms: u64,
    pub populated_regions: usize,
}

impl VisualState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            regions: vec![None; usize::from(REGION_COUNT)],
            applied_atoms: 0,
            rejected_base: 0,
        }
    }
    pub fn apply_surface(&mut self, header: Header, surface: Surface, now: Instant) -> bool {
        let slot = &mut self.regions[usize::from(header.region_id)];
        if let Some(current) = slot.as_ref() {
            if header.base_state_id != 0 && header.base_state_id != current.state_id {
                self.rejected_base += 1;
                return false;
            }
        }
        *slot = Some(RegionState {
            state_id: header.state_id,
            surface,
            updated_at: now,
            confidence: 255,
        });
        self.applied_atoms += 1;
        true
    }
    #[must_use]
    pub fn surface(&self, id: u16) -> Option<&Surface> {
        self.regions
            .get(usize::from(id))
            .and_then(Option::as_ref)
            .map(|state| &state.surface)
    }
    #[must_use]
    pub fn metrics(&self, now: Instant) -> StateMetrics {
        let mut ages = self
            .regions
            .iter()
            .flatten()
            .map(|r| now.saturating_duration_since(r.updated_at).as_millis() as u64)
            .collect::<Vec<_>>();
        ages.sort_unstable();
        let count = ages.len();
        let mean = if count == 0 {
            0
        } else {
            ages.iter().sum::<u64>() / count as u64
        };
        let p95 = if count == 0 {
            0
        } else {
            ages[(count - 1) * 95 / 100]
        };
        StateMetrics {
            applied_atoms: self.applied_atoms,
            rejected_base: self.rejected_base,
            mean_age_ms: mean,
            p95_age_ms: p95,
            populated_regions: count,
        }
    }
    #[must_use]
    pub fn age(&self, id: u16, now: Instant) -> Option<Duration> {
        self.regions
            .get(usize::from(id))
            .and_then(Option::as_ref)
            .map(|r| now.saturating_duration_since(r.updated_at))
    }
}
impl Default for VisualState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fcst_protocol::AtomType;
    fn h(id: u16, state: u32, base: u32) -> Header {
        Header {
            atom_type: AtomType::Surface,
            flags: 0,
            session_epoch: 1,
            atom_sequence: 1,
            frame_tick: 1,
            region_id: id,
            fragment_index: 0,
            fragment_count: 1,
            state_id: state,
            base_state_id: base,
            capture_time_ms: 0,
            ttl_ms: 120,
        }
    }
    fn s() -> Surface {
        Surface {
            quantization: 1,
            luma: [123; 48],
            chroma_a: [0; 12],
            chroma_b: [0; 12],
        }
    }
    #[test]
    fn applies_and_rejects_wrong_base() {
        let mut state = VisualState::new();
        let now = Instant::now();
        assert!(state.apply_surface(h(0, 1, 0), s(), now));
        assert!(!state.apply_surface(h(0, 2, 9), s(), now));
        assert_eq!(state.metrics(now).rejected_base, 1);
    }
}
