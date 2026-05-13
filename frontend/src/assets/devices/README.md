# Device Images

Place PNG files here to display custom device images in the Devices page.

## Naming Convention

Files must be named exactly as:

```
<vid>_<pid>.png
```

Where:
- `<vid>` = USB Vendor ID in **lowercase hex** (4 digits, zero-padded)
- `<pid>` = USB Product ID in **lowercase hex** (4 digits, zero-padded)

## Examples

| Device | PNG filename |
|--------|--------------|
| Logitech G502 Hero | `046d_c539.png` |
| Razer DeathAdder V3 | `1532_c002.png` |
| SteelSeries Arctis 7+ | `1038_12aa.png` |
| Corsair K70 RGB | `1b1c_1b1e.png` |

## Finding VID/PID

```bash
# List USB devices with IDs
lsusb

# Example output:
# Bus 002 Device 003: ID 046d:c539 Logitech, Inc. G502 HERO Gaming Mouse
#                                ↑ VID   PID
```

## Notes

- Images are discovered at **build time** via Vite's `import.meta.glob`.
- After adding new images, restart the dev server or rebuild.
- Use `scripts/fetch_device_assets.sh` (if available) to auto-download known device images.
- Fallback SVG placeholders are used when no PNG matches.
