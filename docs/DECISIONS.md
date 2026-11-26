# Decisions

## 2026-11-26 — Cap the theme crossfade snapshot

Keep at most 2560 × 1600 device pixels (4,096,000 pixels, 15.625 MiB at four bytes per pixel).
Above that area, draw the new palette immediately rather than downscaling or keeping the old
frame. A 3840 × 2160 snapshot alone is about 31.6 MiB and exceeds the 30 MiB private-memory
budget. The cap limits this allocation; it does not establish that total private memory is
under budget.
