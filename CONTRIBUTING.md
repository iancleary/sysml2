# Contributing

Contributions are welcome.

Use a recent stable Rust toolchain. The main development commands are:

```sh
cargo run -- check examples/vehicle.sysml
cargo run -- validate --profile sysml-2.0-requirements-structure-v1 path/to/model
```

Before opening a pull request, run:

```sh
just check
```

Keep changes focused and preserve the documented CLI and JSON compatibility
contracts in [`AGENTS.md`](AGENTS.md).
