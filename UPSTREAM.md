# Upstream provenance

This repository is a hard, purpose-specific fork of selected filesystem
snapshot mechanics from:

- project: `anomalyco/rift`
- repository: <https://github.com/anomalyco/rift>
- pinned commit: `757a22cb247f9b24a849c9d6bd56f49c0ec494f8`
- upstream package version: `0.0.10`
- upstream declared license: `MIT` in `Cargo.toml` and `README.md`

The original public workspace manager, CLI, JavaScript/Bun/Node FFI, registry,
hooks, markers, Git policy, filtered-copy behavior and source-conversion logic
were removed. Subsequent development is specific to Greppy and is not intended
to preserve Rift APIs or merge compatibility.

