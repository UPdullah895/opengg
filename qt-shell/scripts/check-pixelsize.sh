#!/bin/sh
# Check for fractional font.pixelSize values which cause hangs on this system (Qt 6.11.1)
# This implements plan risk R1 mitigation

if grep -rEn 'pixelSize:[[:space:]]*[0-9]+\.[0-9]' qml/ > /dev/null 2>&1; then
    echo "FAIL: fractional font.pixelSize values found:"
    grep -rEn 'pixelSize:[[:space:]]*[0-9]+\.[0-9]' qml/
    exit 1
else
    echo "OK: no fractional font.pixelSize"
    exit 0
fi
