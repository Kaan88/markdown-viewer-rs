# mdview

A Rust remake of Kaan88/markdown-viewer-cpp, a Markdown viewer with a custom renderer. The shell being built is winit + softbuffer with every control hand-drawn (`bins/mdv-app`, binary `mdview`); the Qt Widgets shell (`bins/mdv-qt`) still ships until that one reaches parity. The WinUI 3 shell is retired and archived on the `winui-archive` branch.

Before doing anything, read `docs/HANDOFF.md`: current state, decisions already made, and what to do next. The full plan is `docs/PLAN.md`.

- Commit and push only as phases complete or when asked. There is no CI, so run the tests yourself.
- `spikes/` is throwaway code, kept for reference.
- The user approves the interface by looking at it; screenshots and tests do not stand in for that.
- Report results plainly, including what was not run. Nothing counts as working on Windows until it has run on Windows.
