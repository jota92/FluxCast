# ADR-004: WebGPU has a CPU reference fallback during the first SURFACE sprint

## Status

Accepted for the initial sprint only.

## Decision

The sender defines a WebGPU region-error kernel and requires WebGPU in strict
preflight. The current browser implementation keeps the equivalent 60×45
SURFACE analysis in a Worker as a transparent reference path while the GPU
buffer/readback benchmark is completed.

## Consequence

Strict mode cannot claim FHD30 certification until the GPU benchmark reports
average processing below 20 ms and p95 below 28 ms on each target device. It
must fail preflight rather than silently reduce resolution or frame rate.
