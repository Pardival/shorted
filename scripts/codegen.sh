#!/usr/bin/env bash
# Regenerates the Dart and Rust bridge code. Needs flutter_rust_bridge_codegen 2.13.0.
set -euo pipefail

cd "$(dirname "$0")/../app"
flutter_rust_bridge_codegen generate
