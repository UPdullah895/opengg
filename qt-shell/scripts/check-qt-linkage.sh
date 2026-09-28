#!/bin/sh
# Verify Qt6 is DYNAMICALLY linked into the binary (LGPLv3 compliance — plan §5.1).
# The failure mode we guard against is a statically-linked Qt: a binary that
# contains no libQt6*.so dependencies at all. We assert that the core Qt libraries
# cxx-qt links directly (Core, Gui, Qml) appear as dynamic dependencies in ldd.
#
# NOTE: we deliberately do NOT "fall back" to checking whether the .so exists in
# /usr/lib — Qt is installed on every build box, so that check would pass a
# statically-linked binary and defeat the purpose. The assertion must be about the
# BINARY's linkage, not about what happens to be installed on the system.
#
# libQt6Quick is intentionally not required here: it is dlopen'd at runtime via the
# QML module machinery rather than being a direct DT_NEEDED entry, so a runtime
# /proc/<pid>/maps check (not this static check) is the place to assert it.
# Usage: check-qt-linkage.sh [path-to-binary]   (default: target/debug/opengg-qt)

BIN="${1:-target/debug/opengg-qt}"

if [ ! -f "$BIN" ]; then
    echo "FAIL: binary not found at $BIN"
    exit 1
fi

LDD_OUT=$(ldd "$BIN" 2>/dev/null)

# If NO Qt libraries appear in ldd at all, Qt is statically linked (or missing) —
# the exact condition §5.1 forbids.
if ! printf '%s\n' "$LDD_OUT" | grep -q 'libQt6'; then
    echo "FAIL: no libQt6* dynamic dependencies found — Qt appears statically linked."
    echo "      LGPLv3 requires dynamic linking (users must be able to relink against a replaced Qt)."
    exit 1
fi

MISSING=0
for lib in libQt6Core libQt6Gui libQt6Qml; do
    if printf '%s\n' "$LDD_OUT" | grep -q "$lib\\.so"; then
        echo "OK: $lib dynamically linked"
    else
        echo "FAIL: $lib is not a dynamic dependency of $BIN"
        MISSING=1
    fi
done

if [ "$MISSING" -eq 0 ]; then
    echo "OK: Qt dynamically linked (LGPLv3 §5.1 satisfied)"
    echo "--- Qt libraries in ldd ---"
    printf '%s\n' "$LDD_OUT" | grep 'libQt6'
    exit 0
else
    echo "FAIL: Qt not properly dynamically linked"
    exit 1
fi
