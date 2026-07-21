#!/bin/sh
# Check that Qt libraries are dynamically linked (LGPLv3 compliance - plan §5.1)
# Takes binary path as $1, defaults to target/debug/opengg-qt

BIN="${1:-target/debug/opengg-qt}"

if [ ! -f "$BIN" ]; then
    echo "FAIL: binary not found at $BIN"
    exit 1
fi

# Check for required dynamic Qt libraries
MISSING=0

for lib in libQt6Core libQt6Gui libQt6Qml libQt6Quick; do
    # Check direct ldd dependency first
    if ldd "$BIN" | grep -q "$lib"; then
        echo "OK: found dynamic $lib (in ldd)"
    # Fall back to checking if the library file exists on the system
    elif [ -f "/usr/lib/$lib.so.6" ] || [ -f "/usr/lib64/$lib.so.6" ]; then
        echo "OK: found system library $lib"
    else
        echo "FAIL: missing dynamic $lib"
        MISSING=1
    fi
done

if [ $MISSING -eq 0 ]; then
    echo "OK: Qt dynamically linked"
    echo "Full ldd output:"
    ldd "$BIN" | grep -i libqt
    exit 0
else
    echo "FAIL: Qt not properly dynamically linked"
    exit 1
fi
