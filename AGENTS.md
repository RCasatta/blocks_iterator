# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace centered on two crates:

- `lib/`: the `blocks_iterator` library. Core logic lives in `lib/src/`, with pipeline stages under `lib/src/stages/` and UTXO backends under `lib/src/utxo/`.
- `cli/`: the `blocks_iterator_cli` binary and runnable examples in `cli/examples/`.

Supporting files:

- `blocks/`: small testnet fixture data used by examples and CI.
- `benches/`: standalone benchmark crate, excluded from the main workspace.
- `.github/workflows/rust.yml`: CI source of truth for formatting, linting, tests, examples, and Nix builds.

## Development Environment
The repo is Nix-based (`flake.nix`) and `.envrc` loads it through `nix-direnv`. Prefer `direnv allow` once, then work inside that environment.

When the Nix environment is not already active, prefer `direnv exec . <command>` over `nix develop --command <command>` because it reuses the cached shell. Example: `direnv exec . cargo test --no-default-features`.

## Build, Test, and Development Commands
Use the pinned toolchain from `rust-toolchain.toml`.

- `cargo build --workspace`: build the library and CLI.
- `cargo test --no-default-features`: baseline test run used in CI.
- `cargo test --features db,redb,consensus`: full feature test run on supported toolchains.
- `cargo run -p blocks_iterator_cli -- --blocks-dir blocks --network testnet --max-reorg 40`: run the CLI against the bundled fixture data.
- `cargo run --example outputs_versions -- --blocks-dir blocks --network testnet`: run a sample example.
- `cargo fmt -- --check` and `cargo clippy -- -D warnings`: match CI formatting and lint gates.
- `nix build .`: build through the flake-based environment.

## Coding Style & Naming Conventions
Follow standard Rust style: 4-space indentation, `snake_case` for modules/functions/files, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Keep modules focused; stage-specific logic belongs in `lib/src/stages/`, storage backends in `lib/src/utxo/`. Run `cargo fmt` before submitting and treat clippy warnings as errors.

## Testing Guidelines
Tests are primarily inline unit tests beside the implementation (`#[cfg(test)]` blocks in `lib/src/**`). Add tests close to the code you change. Cover feature-specific behavior with the corresponding feature enabled. When updating examples or pipe behavior, run them against `blocks/`.

## Commit & Pull Request Guidelines
Recent history favors short, imperative commit messages such as `clippy fixes`, `bump version 2.1.1 -> 2.1.2`, or scoped subjects like `cli: specify use lib 2.1.2`. Keep commits small and descriptive.

Pull requests should include a concise summary, note any feature flags affected, and list the commands you ran locally. If output or CLI behavior changes, include a short sample invocation and result.
