# VaultSearch
[![CI](https://github.com/your-org/VaultSearch/actions/workflows/ci.yml/badge.svg)](https://github.com/your-org/VaultSearch/actions/workflows/ci.yml)

VaultSearch is a privacy-focused, offline search utility that builds a local Tantivy index over a folder of text-like files. It provides a simple command-line interface to initialize configuration, index your files, and search for matches without sending data anywhere else.

## Features
- Config-driven indexing rooted at a directory you choose.
- Fast full-text search powered by [Tantivy](https://tantivy-search.github.io/).
- Skips non-text file types to avoid noisy results.
- Clear progress reporting during indexing.

## Requirements
- Rust 1.70+ (edition 2021)
- A filesystem location that you can read and has enough space for the index.

## Installation

### Download a release archive (recommended)
1. Grab the latest archive for your platform from the Releases section (naming pattern: `vaultsearch-<VERSION>-<TARGET>.tar.gz` for Unix-like systems, `vaultsearch-<VERSION>-<TARGET>.zip` for Windows).
2. Extract it somewhere on your `PATH` (or copy the binary to a directory on your `PATH`).
   ```bash
   tar -xzf vaultsearch-<VERSION>-x86_64-unknown-linux-gnu.tar.gz
   sudo mv vaultsearch-<VERSION>-x86_64-unknown-linux-gnu/vaultsearch /usr/local/bin/
   vaultsearch --version
   ```
3. On Windows, unzip the archive and run the binary from PowerShell so you can see its output:
   ```powershell
   Expand-Archive -Path vaultsearch-<VERSION>-x86_64-pc-windows-gnu.zip -DestinationPath .
   .\vaultsearch-<VERSION>-x86_64-pc-windows-gnu\vaultsearch.exe --help
   ```

### Build from source
```bash
# From the repository root
cargo build --release
# The binary will be at target/release/vaultsearch (or target/release/vaultsearch.exe on Windows)
```

### Upgrading
- Download the newer release archive, replace your existing `vaultsearch` binary with the new one, and rerun `vaultsearch --version` to confirm the upgrade.
- Your configuration and index data live under your OS-specific config/data directories, so replacing the binary does not delete or reset your existing index. If a release changes the indexing format, rerun `vaultsearch index` after upgrading.

## Usage
Run `vaultsearch --help` to see all options. The typical workflow looks like this:

1. **Initialize** configuration and index location
   ```bash
   vaultsearch init --root /path/to/documents
   ```
   This stores a `config.toml` in your platform's configuration directory (e.g., `~/.config/vaultsearch`) and a Tantivy index under your platform's data directory (e.g., `~/.local/share/vaultsearch/index`).

2. **Index** the files under your root directory
   ```bash
   vaultsearch index
   ```
   Progress is printed in batches so you can monitor indexing throughput.

3. **Search** for terms
   ```bash
   vaultsearch search "invoice 2024"
   ```
   The top-ranked results (by score) are printed as a human-readable list that includes the rank, score, relative path (if it
   lives under your configured root), and a highlighted text snippet.

## Configuration
The tool stores configuration and index data using your OS-specific directories (provided by the `directories` crate). On most systems you can find:
- `config.toml` under the user configuration directory (e.g., `~/.config/vaultsearch`).
- Index data under the user local data directory (e.g., `~/.local/share/vaultsearch/index`). These are separate directories: the
  configuration file is not stored alongside the Tantivy index.

You can edit `config.toml` manually if you need to change the root or index location, or rerun `vaultsearch init` with a different `--root` to recreate it.

## Release artifacts and reproducible builds
- Run `scripts/package-release.sh` from the repository root to produce platform-specific archives under `dist/`. The script pins dependencies via `Cargo.lock` (`--locked`) and reuses the shared `target/` directory so repeated runs emit consistent outputs.
- Set the `TARGETS` environment variable to customize the build matrix (default targets: `x86_64-unknown-linux-gnu x86_64-pc-windows-gnu aarch64-apple-darwin`).
- Each archive contains the `vaultsearch` binary alongside `README.md` and `CHANGELOG.md` so consumers can keep documentation in sync with the shipped version.
- Checksums and signatures: the packaging script writes a `.sha256` file next to each archive and, when `SIGN_ARTIFACTS=1` is set, generates an armored detached signature (`.asc`) using `gpg` (optionally controlled by `GPG_SIGNING_KEY`). These live in `dist/` alongside the archives.
- Verification: `cd dist && sha256sum -c vaultsearch-<VERSION>-<TARGET>.tar.gz.sha256` (or `.zip.sha256`) to check the checksum, then `gpg --verify vaultsearch-<VERSION>-<TARGET>.tar.gz.asc` if signatures were produced.
- Prerequisites: the Rust toolchain (with `rustup` for target installation), `jq` for reading cargo metadata, `tar` and `zip` for packaging, and optionally `gpg` for signatures.

## Continuous integration
- GitHub Actions (`ci.yml`) runs `cargo fmt`, `cargo clippy`, and `cargo test` on every push and pull request.
- Pushes also trigger release builds for the supported targets listed above. Archives, checksum files, and (on tags) optional signatures are uploaded as workflow artifacts. Set `GPG_PRIVATE_KEY`, `GPG_PASSPHRASE` (if needed), and `GPG_SIGNING_KEY` secrets to enable signing.


## Release notes and versioning
- VaultSearch follows semantic versioning; the current release is **0.2.0** (see `Cargo.toml`).
- Each release is described in `CHANGELOG.md`. When publishing a new version, update the changelog entry and increment the crate version so customers can match downloaded binaries to documented changes.

## Development
- Format code with `cargo fmt`.
- Run checks with `cargo test`.

## Notes
This project aims to stay offline-first. Keep your index on trusted storage and run searches locally to maintain privacy.

## License
VaultSearch is licensed under the MIT License. See [LICENSE](LICENSE) for details about the permissions and limitations.
