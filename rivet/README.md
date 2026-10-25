# Rivet implementation

This directory is the only home for Rivet code and packaging inputs.

The initial repository contains no Rust crate because the implementation requirements remain
`Draft`. The first implementation change creates the relevant crate below this directory, its
manifest, a failing requirement-named test and the smallest code that makes the test pass.

Planned homes are recorded in `AGENTS.md`:

- `protocol/`
- `core/`
- `runtime/windows/`
- `platform/windows/`
- `cli/`
- `ember/`
- `installer/windows/`
