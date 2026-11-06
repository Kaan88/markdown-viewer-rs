# mdview

A Rust remake of Kaan88/markdown-viewer-cpp, a Markdown viewer with a custom renderer. WinUI 3 on Windows 11, Qt Widgets through CXX-Qt on Linux.

Before doing anything, read `docs/HANDOFF.md`: current state, decisions already made, and what to do next. The full plan is `docs/PLAN.md`.

- Commit and push only as phases complete or when asked. There is no CI, so run the tests yourself.
- `spikes/` is throwaway code, kept for reference.
- Report results plainly, including what was not run. Nothing counts as working on Windows until it has run on Windows.
