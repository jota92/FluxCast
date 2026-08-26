# ADR-002: No standard video codec in the initial visual path

## Status

Accepted.

## Decision

The initial path represents camera input as FVSC region updates and FCST
visual atoms. It does not use a conventional video codec or its transport
semantics.

## Consequences

The PoC prioritizes state convergence, timing, and measurable freshness over
compression efficiency. Any change to this decision requires an ADR with
experiment conditions, measurements, cause, alternatives, and requirement
impact.
