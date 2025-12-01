#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
TARGETS=${TARGETS:-"x86_64-unknown-linux-gnu x86_64-pc-windows-gnu aarch64-apple-darwin"}

VERSION=$(python - <<'PY'
import json
import subprocess

metadata = json.loads(subprocess.check_output([
    "cargo",
    "metadata",
    "--no-deps",
    "--format-version",
    "1",
]))

package = next(pkg for pkg in metadata["packages"] if pkg["name"] == "vaultsearch")
print(package["version"])
PY
)

echo "Building VaultSearch ${VERSION} for targets: ${TARGETS}" >&2
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

for target in $TARGETS; do
  rustup target add "$target" >/dev/null

  echo "\n==> Compiling for ${target}" >&2
  CARGO_TARGET_DIR="$ROOT_DIR/target" cargo build --release --locked --target "$target"

  artifact_root="$DIST_DIR/vaultsearch-${VERSION}-${target}"
  mkdir -p "$artifact_root"

  bin_name="vaultsearch"
  [[ "$target" == *"windows"* ]] && bin_name="vaultsearch.exe"

  cp "$ROOT_DIR/target/${target}/release/${bin_name}" "$artifact_root/"
  cp "$ROOT_DIR/README.md" "$ROOT_DIR/CHANGELOG.md" "$artifact_root/"

  pushd "$DIST_DIR" >/dev/null
  if [[ "$target" == *"windows"* ]]; then
    zip -rq "$(basename "$artifact_root").zip" "$(basename "$artifact_root")"
  else
    tar -czf "$(basename "$artifact_root").tar.gz" "$(basename "$artifact_root")"
  fi
  popd >/dev/null

  rm -rf "$artifact_root"
done

echo "\nArtifacts ready under ${DIST_DIR}" >&2
