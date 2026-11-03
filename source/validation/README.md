# Selected game source

`src/lib.rs` validates and hashes the selected files. `src/main.rs` is the
manifest command: `cargo run -p validation --locked -- <install-root> <private-manifest>`.
Every gameplay session validates the selected source. Cached names and remembered
paths cannot admit a build. Keep the installation read-only.

See the [folder guide](../docs/folder-guide.md) and [architecture](../ARCHITECTURE.md).
