# Simulated User Stability Testing

LocalConvert Desktop includes a deterministic, offline workload harness for development stability checks. It exercises production Rust execution paths with synthetic files under one unique operating-system temporary directory.

This is simulated workload evidence, not real-user research or proof of production usage.

## Profiles

| Profile | Virtual sessions | Real task attempts | Concurrency | Default seed |
| --- | ---: | ---: | --- | --- |
| quick | 20 | 300 | 1, 2, 4 | `0x4c43443039300001` |
| standard | 100 | 4,000 | 1, 2, 4, 8 | `0x4c43443039300002` |
| stress | 400 | up to 20,000 | 4, 8 | `0x4c43443039300003` |
| soak | repeated quick waves | 120 minutes by default | 1, 2, 4 | `0x4c43443039300004` |

Run the normal profiles:

```bash
npm run test:simulated-users:quick
npm run test:simulated-users:standard
```

Heavy profiles require explicit authorization and at least 20 GiB of free space:

```bash
npm run test:simulated-users:stress -- --allow-stress --seed 0x4c43443039300003
npm run test:simulated-users:soak -- --allow-soak --duration-minutes 120 --seed 0x4c43443039300004
```

Each failed campaign prints its exact seed and rerun command. Optional controls include `--concurrency`, `--task-timeout`, `--scenario-timeout`, `--max-data-gb`, `--dry-run`, and `--discard-on-failure`.

## Coverage

- PDF merge, split, rotation, and page extraction through bundled qpdf.
- Image conversion, resize, compression, and metadata cleanup through the bundled image engine.
- Output location and naming rules, collision handling, cancellation, retry records, preferences, and filtered CSV/JSON reports.
- Unicode, spaces, parentheses, multi-dot names, long names, nested folders, read-only sources, invalid files, missing files, and unsupported files.
- A deterministic Vitest workload with at least 10,000 frontend task records.

The harness verifies source SHA-256 values, no-overwrite output publication, unique final paths, cancellation/failure cleanup, report privacy fields, no orphan task temporary directories, and no retained child processes.

## Safety Gates

- All generated input and output data stays below one unique `<simulation-root>` in the operating-system temp directory.
- Cleanup rejects any target outside that exact root.
- A profile stops if free space drops below 10 GiB, generated data exceeds 10 GB or the configured lower cap, severe memory pressure is observed, or repeated crashes/timeouts occur.
- Disk-full behavior is boundary simulated; the harness never intentionally fills the real disk.
- Commands use argument arrays and do not add network access, upload behavior, or shell-string execution.

Generated summaries are stored in:

- `reports/simulated-users/latest-summary.md`
- `reports/simulated-users/latest-results.json`

Reports replace absolute test paths with `<repo-root>` or `<simulation-root>` and explicitly label unavailable measurements.
