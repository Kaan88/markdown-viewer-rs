# State

See [HANDOFF.md](HANDOFF.md) for the branch history and current implementation.

## Motion: open points

- The 117 ms Settings entrance still takes a whole-window snapshot on each blended frame.
  Reducing that allocation without changing how the dim and card fade together needs a shared
  crate change. The shared pin is unchanged.
- The theme crossfade now caps its snapshot at 15.625 MiB. Total private-memory peak against the
  30 MiB budget is not verified.
