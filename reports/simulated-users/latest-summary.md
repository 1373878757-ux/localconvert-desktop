# Simulated User Stability Summary

> This is deterministic simulated workload evidence, not evidence from real users. Absolute paths are omitted or replaced with `<repo-root>` and `<simulation-root>`.

- **Generated:** 2026-07-22T17:16:30.295Z
- **Verdict:** PASS
- **Covered:** 120 virtual sessions and 4300 real backend task attempts
- **Invariant violations:** 0
- **Crashes / timeouts:** 0 / 0

## Profiles

| Profile | Seed | Sessions | Attempts | Concurrency | Duration | Verdict |
| --- | --- | ---: | ---: | --- | ---: | --- |
| quick | 0x4c43443039300001 | 20 | 300 | 1, 2, 4 | 21212 ms | pass |
| standard | 0x4c43443039300002 | 100 | 4000 | 1, 2, 4, 8 | 210899 ms | pass |

## Operation Distribution

| Operation | Attempts |
| --- | ---: |
| image-clean-metadata | 453 |
| image-compress | 636 |
| image-convert | 1055 |
| image-resize | 649 |
| pdf-extract | 372 |
| pdf-merge | 436 |
| pdf-rotate | 366 |
| pdf-split | 333 |

## Status Distribution

| Status | Attempts |
| --- | ---: |
| cancelled | 86 |
| failed | 115 |
| not_smaller | 103 |
| skipped | 158 |
| success | 3800 |
| unsupported | 38 |

## Resource Observations

- **quick:** peak RSS 16.0 MiB, peak CPU 32.1%, peak FDs 10, peak child processes unavailable, peak temporary data 1.2 MiB.
- **standard:** peak RSS 17.0 MiB, peak CPU 109.6%, peak FDs 14, peak child processes unavailable, peak temporary data 3.7 MiB.

Unavailable measurements remain explicit in the JSON report. Resource data is sampled from local `ps`, `lsof`, `df`, and macOS memory tools without shell command strings.

## Cold Launch Results

| Launch | Main-window time | Splash | Main window | qpdf | image-engine | No startup error |
| ---: | ---: | --- | --- | --- | --- | --- |
| 1 | 2502 ms | yes | yes | yes | yes | yes |
| 2 | 2349 ms | yes | yes | yes | yes | yes |
| 3 | 2419 ms | yes | yes | yes | yes | yes |
| 4 | 2447 ms | yes | yes | yes | yes | yes |
| 5 | 2339 ms | yes | yes | yes | yes | yes |
| 6 | 2357 ms | yes | yes | yes | yes | yes |
| 7 | 2383 ms | yes | yes | yes | yes | yes |
| 8 | 2348 ms | yes | yes | yes | yes | yes |
| 9 | 2301 ms | yes | yes | yes | yes | yes |
| 10 | 2328 ms | yes | yes | yes | yes | yes |

## GUI Smoke Results

| Check | Result | Note |
| --- | --- | --- |
| native file picker | pass | A public synthetic showcase image was selected through the native macOS picker and entered the queue as a native-path task. |
| packaged image conversion | pass | One PNG-to-WebP task completed through the packaged image-engine; the UI showed its output and no error. |
| source integrity | pass | The selected source retained SHA-256 c812ac6fcc7bfb261f297132c3900ab37b5276d4f0b5408b42b0b0ca94b1a6a6. |
| task temporary cleanup | pass | No .localconvert-task-* directory or orphan qpdf/image-engine process remained after the operation. |
| packaged-app network | pass | lsof reported no TCP or UDP endpoint for the packaged application during the local operation |
| GUI matrix scope | limited by design | Only a native-picker and image-conversion main path were repeated in GUI; the 4,300-task Rust campaigns cover the full operation mix. |

UI automation restrictions are reported separately and are not treated as product failures.

## Safety Invariants

- Source SHA-256 changes: 0.
- Output overwrite violations: 0.
- Temporary directory leaks: 0.
- Network observation: lsof reported no TCP or UDP endpoint for the packaged application during the local operation.

## Deferred Heavy Profiles

- **stress:** Deferred until at least 20 GiB free disk space is available and --allow-stress is explicit.
  `npm run test:simulated-users:stress -- --allow-stress --seed 0x4c43443039300003`
- **soak:** Deferred until at least 20 GiB free disk space is available and --allow-soak is explicit.
  `npm run test:simulated-users:soak -- --allow-soak --duration-minutes 120 --seed 0x4c43443039300004`
