#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p www/fonts
cp ../data/rickview.css ../data/roboto.css ../data/favicon.ico www/
cp ../fonts/roboto300.woff2 www/fonts/
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128"
wasm-pack build --target web --release --out-dir www/pkg
