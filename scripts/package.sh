#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
cargo build --release --locked -p cad-cli

platform="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
package_name="headless-cad-$platform"
package_stage="$(mktemp -d "${TMPDIR:-/tmp}/headless-cad-package.XXXXXX")"
trap 'rm -rf "$package_stage"' EXIT
mkdir -p "$package_stage/$package_name" "$project_root/dist"
cp "${CARGO_TARGET_DIR:-$project_root/target}/release/cad" "$package_stage/$package_name/cad"
cp README.md LICENSE "$package_stage/$package_name/"
cp -R docs "$package_stage/$package_name/docs"
cp -R examples "$package_stage/$package_name/examples"
tar -czf "$project_root/dist/$package_name.tar.gz" -C "$package_stage" "$package_name"
printf '%s\n' "$project_root/dist/$package_name.tar.gz"
