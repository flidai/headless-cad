#!/usr/bin/env bash
# Serve every example and rebuild its artifacts whenever its recipe changes.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
if [[ -z "${CAD_BIN:-}" ]]; then
  cargo build --locked -p cad-cli
  cad_binary="${CARGO_TARGET_DIR:-$project_root/target}/debug/cad"
else
  cad_binary="$CAD_BIN"
fi
children=()
cleanup() {
  trap - EXIT INT TERM
  if ((${#children[@]})); then
    kill "${children[@]}" 2>/dev/null || true
    wait "${children[@]}" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p out
# Keep the last complete generations available while examples regenerate.
"$cad_binary" serve out "$@" &
children+=("$!")
for recipe in examples/*.star; do
  name="$(basename "$recipe" .star)"
  "$cad_binary" build "$recipe" --out-dir "out/$name" --cache-dir .cad-cache >/dev/null
  "$cad_binary" watch "$recipe" --out-dir "out/$name" --cache-dir .cad-cache &
  children+=("$!")
done
# Parameter presets keep their authoritative recipes and captured overrides.
# The server serves artifacts only; watchers own recipe evaluation.
watch_preset() {
  local name="$1" recipe="$2"
  shift 2
  "$cad_binary" build "$recipe" "$@" --out-dir "out/$name" --cache-dir .cad-cache >/dev/null
  "$cad_binary" watch "$recipe" "$@" --out-dir "out/$name" --cache-dir .cad-cache &
  children+=("$!")
}
watch_preset drive_module_pocketed examples/drive_module_rotation.star --param base_pocket_depth=1
watch_preset drive_module_pocketed_tightened examples/drive_module_tightened.star --param base_pocket_depth=1
watch_preset drive_module_pocketed_adjustment examples/drive_module_adjustment.star --param base_pocket_depth=1
# Stop the complete development session if any long-lived child exits.
wait -n "${children[@]}"
