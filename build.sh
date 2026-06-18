#!/usr/bin/env bash
. "$HOME/.cargo/env"
set -euo pipefail

cd "$(dirname "$0")"

ARCH="${ARCH:-x86_64}"

case "$ARCH" in
    x86_64)
        TARGET="x86_64-unknown-none"
        FEATURES="arch_x86_64"
        ;;
    aarch64)
        TARGET="aarch64-unknown-none"
        FEATURES="arch_aarch64"
        ;;
    *)
        echo "Unsupported ARCH: $ARCH (use x86_64 or aarch64)"
        exit 1
        ;;
esac

echo "Building Aperture OS kernel for $ARCH..."
# The x86_64 kernel uses naked assembly that references static symbols; the
# large code model prevents R_X86_64_32S linker relocations in that context.
cargo build -p kernel --no-default-features --features "$FEATURES" \
    -Z build-std=core,compiler_builtins,alloc \
    -Z build-std-features=compiler-builtins-mem \
    --target "$TARGET"

KERNEL_ELF="target/$TARGET/debug/kernel"

if [[ "$ARCH" == "x86_64" ]]; then
    UEFI_IMAGE="target/aperture-uefi.img"
    BIOS_IMAGE="target/aperture-bios.img"
    mkdir -p target

    echo "Building boot images..."
    cargo run --manifest-path tools/bootimage/Cargo.toml -- "$KERNEL_ELF" "$UEFI_IMAGE" "$BIOS_IMAGE"

    echo "UEFI image: $UEFI_IMAGE"
    echo "BIOS image: $BIOS_IMAGE"
else
    echo "Building AArch64 boot image..."
    ISO_IMAGE="target/aperture-aarch64.iso"
    tools/aarch64-bootimage.sh "$KERNEL_ELF" "$ISO_IMAGE"
    echo "AArch64 boot image: $ISO_IMAGE"
fi
