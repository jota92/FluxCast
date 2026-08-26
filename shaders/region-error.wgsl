// FC-006/FC-010 reference kernel: one invocation per 32×24 Region.
// The Sender currently retains a CPU reference path in analysis-worker.ts;
// this kernel is the WebGPU fast-path contract and benchmark target.
struct RegionMetric { changed_luma: f32, edge_energy: f32, motion_hint: f32, _reserved: f32 };
@group(0) @binding(0) var current_luma: texture_2d<f32>;
@group(0) @binding(1) var previous_luma: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<RegionMetric>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= 60u || id.y >= 45u) { return; }
  let origin = vec2<i32>(i32(id.x * 32u), i32(id.y * 24u));
  var difference = 0.0;
  var edges = 0.0;
  for (var y = 0; y < 24; y = y + 1) {
    for (var x = 0; x < 32; x = x + 1) {
      let p = origin + vec2<i32>(x, y);
      let value = textureLoad(current_luma, p, 0).r;
      difference = difference + abs(value - textureLoad(previous_luma, p, 0).r);
      if (x > 0) { edges = edges + abs(value - textureLoad(current_luma, p - vec2<i32>(1, 0), 0).r); }
    }
  }
  let region = id.y * 60u + id.x;
  output[region] = RegionMetric(difference / 768.0, edges / 744.0, 0.0, 0.0);
}
