# Changelog — CircuitCraft

All notable changes to CircuitCraft are documented here.

## [Unreleased]

Productisation slice toward v0.2.0. No tag or release is created here; the
project format stays `circuitcraft`, schema 1.

### Added

- Component rotation in 90° steps (`R` key, toolbar and inspector), stored as a
  serde-defaulted `rotation` field; pins and symbols rotate with the component.
- Orthogonal wire routing with automatic junction detection at shared pins and
  crossings; individual wires can be selected and deleted.
- Named nets (`GND`, `V+`, `N<n>`) from union-find over pins and junctions.
- PDF schematic export alongside CSV and SVG.
- Grid snapping (20 units, toggleable), zoom-to-fit (`F`) and a richer palette
  with transistor, op amp and connector symbols.
- v0.2 JSON fields use `#[serde(default)]`: v0.1.0 documents open unchanged and
  no schema migration is required.
- Domain tests for rotation, orthogonal routes, junction merging, named nets,
  SVG/PDF validity and escaping, CSV, round-trip and undo/redo.

## [0.1.0] - 2026-11-22

### Added

- Initial CraftKit-based application shell.
- Versioned project files with atomic save.
- Undo/redo through the shared document history.
- Background jobs with progress and cancellation.
- Application-specific document model and UI.
