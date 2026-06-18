#!/usr/bin/env bash
set -euo pipefail

# Build a hybrid BIOS+UEFI bootable ISO from the bootloader disk images.
# Assumes ./build.sh x86_64 has already produced target/aperture-uefi.img
# and target/aperture-bios.img.

UEFI_IMG="target/aperture-uefi.img"
BIOS_IMG="target/aperture-bios.img"
ISO_DIR="target/iso"
OUT_ISO="target/aperture.iso"

for tool in xorriso sfdisk dd; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Error: required tool '$tool' is not installed." >&2
        exit 1
    fi
done

if [[ ! -f "$UEFI_IMG" ]]; then
    echo "Error: $UEFI_IMG not found. Run ./build.sh x86_64 first." >&2
    exit 1
fi

if [[ ! -f "$BIOS_IMG" ]]; then
    echo "Error: $BIOS_IMG not found. Run ./build.sh x86_64 first." >&2
    exit 1
fi

rm -rf "$ISO_DIR"
mkdir -p "$ISO_DIR"
cp "$BIOS_IMG" "$ISO_DIR/aperture-bios.img"

# The UEFI image is a GPT disk. Extract the EFI system partition so it can be
# used as the El Torito EFI boot image.
echo "Extracting UEFI EFI system partition from $UEFI_IMG..."
START_SECTOR=$(sfdisk -d "$UEFI_IMG" | awk -F': ' '/^\S+1 :/ {print $2}' | awk -F', ' '{for(i=1;i<=NF;i++) if($i ~ /^start=/) {gsub(/^start=[[:space:]]*/, "", $i); print $i; exit}}')
PART_SIZE=$(sfdisk -d "$UEFI_IMG" | awk -F': ' '/^\S+1 :/ {print $2}' | awk -F', ' '{for(i=1;i<=NF;i++) if($i ~ /^size=/) {gsub(/^size=[[:space:]]*/, "", $i); print $i; exit}}')

if [[ -z "$START_SECTOR" || -z "$PART_SIZE" ]]; then
    echo "Error: could not parse EFI system partition from sfdisk output." >&2
    exit 1
fi

echo "EFI system partition at sector $START_SECTOR, size $PART_SIZE sectors."
dd if="$UEFI_IMG" of="$ISO_DIR/efi.img" bs=512 skip="$START_SECTOR" count="$PART_SIZE" status=progress

echo "Building hybrid BIOS+UEFI ISO: $OUT_ISO"
xorriso -as mkisofs \
    -r -V 'Aperture OS' \
    -o "$OUT_ISO" \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    -b aperture-bios.img \
    -eltorito-alt-boot -e efi.img -no-emul-boot \
    "$ISO_DIR"

echo "Done: $OUT_ISO"
