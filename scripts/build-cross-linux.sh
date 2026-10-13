#!/usr/bin/env bash
# Build-machine script. End users open BrainTrade.exe instead.
set -euo pipefail
cd "$(dirname "$0")/.."
for braintrade_tool in cargo rustup x86_64-w64-mingw32-gcc python3; do
    command -v "$braintrade_tool" >/dev/null || { echo "Missing build tool: $braintrade_tool"; exit 1; }
done
rustup target add x86_64-pc-windows-gnu
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    cargo build --release --locked --target x86_64-pc-windows-gnu -p braintrade-desktop
python3 -m venv .build-venv
.build-venv/bin/python -m pip install pefile==2024.8.26
.build-venv/bin/python scripts/package-portable.py \
    --exe target/x86_64-pc-windows-gnu/release/BrainTrade.exe
