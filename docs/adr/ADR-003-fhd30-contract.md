# ADR-003: FHD30 is a contract

## Status

Accepted.

## Decision

The Edge renderer operates at 1920×1080 and 30 fps independent of arrival.
When capacity falls, FlexCast reduces atom detail and update density instead of
automatically switching resolution or frame rate.
