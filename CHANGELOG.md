# Changelog — CircuitCraft

All notable changes to CircuitCraft are documented here.

## [Unreleased]

### Added

- Circuit document with seven component kinds, pins and wires, plus a sample
  source → resistor → LED → ground circuit.
- Union-find netlist, BOM grouped by `(kind, value)`, CSV and SVG exporters.
- Schematic editor: palette, pan/zoom canvas, placement, selection, dragging,
  deletion, pin-to-pin wiring, netlist and property panels.
- Domain tests for netlist, deletion cleanup, BOM grouping, JSON round-trip,
  undo/redo of a move and SVG escaping.

## [0.1.0] - 2026-11-22

### Added

- Initial CraftKit-based application shell.
- Versioned project files with atomic save.
- Undo/redo through the shared document history.
- Background jobs with progress and cancellation.
- Application-specific document model and UI.
