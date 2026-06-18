#!/usr/bin/env bash
. "$HOME/.cargo/env"
set -euo pipefail

cd "$(dirname "$0")"

# Build kernel for x86_64 bare metal
cargo build -p kernel -Z build-std=core,compiler_builtins,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --target x86_64-unknown-none

KERNEL_ELF="target/x86_64-unknown-none/debug/kernel"
UEFI_IMAGE="target/aperture-uefi.img"
BIOS_IMAGE="target/aperture-bios.img"

mkdir -p target

echo "Building boot images..."
cargo run --manifest-path tools/bootimage/Cargo.toml -- "$KERNEL_ELF" "$UEFI_IMAGE" "$BIOS_IMAGE"

echo "UEFI image: $UEFI_IMAGE"
echo "BIOS image: $BIOS_IMAGE"
