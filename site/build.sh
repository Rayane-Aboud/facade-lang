#!/usr/bin/env bash
# Build the playground into site/: the checker as WebAssembly (site/pkg) and
# a copy of the example programs (site/examples). Used locally and by the
# GitHub Pages workflow.
set -euo pipefail
cd "$(dirname "$0")/.."

wasm-pack build crates/wasm --target web --release --out-dir ../../site/pkg
rm -rf site/examples
cp -r examples site/examples

echo "built site/  -  preview with: python3 -m http.server -d site 8000"
