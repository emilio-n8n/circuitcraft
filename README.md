# CircuitCraft

Schematic capture with components, wires, netlist and BOM

**CircuitCraft is an independent, community open-source project.** It is not an
official product of, endorsed by, or affiliated with any third-party vendor it
may interoperate with.

Built on [CraftKit](https://github.com/emilio-n8n/craftkit), a small native Rust
desktop foundation.

## Features

- Native Rust desktop UI (egui/eframe), no web runtime.
- Component palette with resistors, capacitors, inductors, diodes, ground,
  voltage sources, switches, transistors, op amps and connectors.
- Pan/zoom canvas with a 20-unit snap grid and zoom-to-fit (`F`): place, select,
  move, rotate (`R`) and delete components; wire two pins by clicking them.
- Orthogonal wire routing (horizontal/vertical segments) with automatic
  junction detection; wires can be selected and deleted independently.
- Live named netlist computed with union-find over wires and junctions, plus an
  editable label/value inspector.
- BOM grouped by `(kind, value)`; CSV bill of materials plus standalone SVG and
  PDF schematic exports.
- Symbols are drawn directly on the egui painter; no PCB or SPICE simulation.
- Versioned JSON project files with atomic saves and bounded undo/redo.
  Documents written by v0.1.0 load unchanged (schema 1 is preserved).
- Offline-first: no account, no telemetry, no network required.

## Run

```bash
cargo run --release -- --sample
cargo run --release -- --open project.json
```

Without arguments the application opens an empty document.

- `--sample` opens the built-in demonstration document.
- `--open <file>` opens a project.
- `--version` prints the version.
- `--help` prints usage.

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Linux windowing development packages are installed by the CI workflow.

## Project layout

```text
src/main.rs       CLI and window startup
src/app.rs        UI, palette, canvas and inspector
src/document.rs   components, pins, wires and document model
src/symbols.rs    pure symbol geometry shared by all renderers
src/logic.rs      wire routes, junctions, netlist, BOM, CSV/SVG/PDF exports
```

The application depends on pinned CraftKit crates from Git. Keep the tag in
`Cargo.toml` and `Cargo.lock` consistent when upgrading the foundation.

## File format

Projects are JSON envelopes:

```json
{
  "format": "circuitcraft",
  "schema_version": 1,
  "app_version": "0.1.0",
  "document": { }
}
```

The application owns the `document` schema and its migrations. A newer
`schema_version` is rejected instead of being silently misread.

## Licence

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.

## Third-party notices

See [THIRD_PARTY.md](THIRD_PARTY.md).
