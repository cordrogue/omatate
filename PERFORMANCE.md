# Historical Python and GTK Rust comparison

These measurements compare the Python GTK panel with the Rust GTK panel on
2026-09-05. They do not measure the Quickshell port or its shared Omarchy shell
process. No Quickshell speed or memory improvement is claimed here.

| Measurement | Python median | Rust median | Improvement |
|---|---:|---:|---:|
| Status command | 49.9 ms | 1.0 ms | 48.2× faster |
| Inactive dictation handoff | 22.9 ms | 2.2 ms | 10.6× faster |
| Render notes | 50.7 ms | 0.9 ms | 54.4× faster |
| Panel startup | 696.0 ms | 308.3 ms | 2.3× faster |
| Panel resident memory | 91.3 MiB | 51.1 MiB | 44% lower |
| Panel proportional memory | 72.0 MiB | 33.8 MiB | 53% lower |

The CLI results use 60 measured runs after five warmups for each command. Dictation used a no-op Voxtype stub, so that result measures handoff overhead, not speech recognition. Rendering used the same two notes and one section in temporary storage.

Panel results use 10 fresh processes per version, alternating their launch order. Startup measures process launch to a successful socket ping, including theme resolution and session loading. Memory was sampled from `/proc/<pid>/smaps_rollup` after a short settling period. Both panels registered zero CPU ticks across the combined three-second idle samples; this does not establish zero CPU use over longer periods.

Python used its existing GTK renderer. The Rust release build used Cairo, its new default. The memory improvement includes that renderer choice. A separate Rust comparison measured about 78 MiB RSS with the default GPU renderer and 52 MiB with Cairo. Explicit `GSK_RENDERER` settings remain supported.

These are local warm-cache measurements, not cold-boot or cross-machine guarantees. Codex analysis latency, actual transcription speed, and large-session workloads were not benchmarked.

Validation passed before replacement: 26 normal Rust tests, four automated GTK checks run on Wayland, strict Clippy, formatting, and the release build.

Raw timings and baseline source hashes are in [benchmarks/results.json](benchmarks/results.json).
