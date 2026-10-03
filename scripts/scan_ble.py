#!/usr/bin/env python3
"""Scan for GAN smart cubes via BLE and display MAC addresses.

Outputs both the BLE device address (from the radio layer) and the MAC
embedded in the manufacturer-specific advertising data (last 6 bytes of
the 11-byte payload -- see ble.rs:214-221).

Dependencies: bleak  (pip install bleak)
"""

import asyncio
import sys

from bleak import BleakScanner

# Exact matches for emulated cubes (emu) -- fallback to prefix match for real cubes.
DEVICE_NAMES = {
    "GANi3SB": "Gen2 (V1/GANi3SB)",
    "GANicV2S_EMU": "Gen3 (V2/GANicV2S)",
    "GANi4_EMU": "Gen4 (V3/GANi4)",
    "GAN251ui_EMU": "Gan251 (V3-2)",
    "WCU_MY32_EMU": "MoYu32 (WCU)",
    "QY-QYSC-A-EMU": "QiYi (Smart)",
}

# Prefixes used by real GAN/MoYu/QiYi cubes (catches both real and emu).
DEVICE_PREFIXES = ("GAN", "WCU", "QY-")


def _mac_str(raw: bytes) -> str:
    return ":".join(f"{b:02X}" for b in raw)


def _on_advertisement(device, adv):
    name = device.name
    if not name or not name.startswith(DEVICE_PREFIXES):
        return

    label = DEVICE_NAMES.get(name, f"Unknown ({name})")
    print(f"[+] {label}  ({device.name})")
    print(f"    BLE device address:         {device.address}")

    for mfr_id, mfr_data in adv.manufacturer_data.items():
        print(f"    Manufacturer ID (CIC):      0x{mfr_id:04X}")
        print(f"    Manufacturer data (hex):     {mfr_data.hex()}")
        if len(mfr_data) >= 6:
            mac_from_mfr_raw = mfr_data[-6:]
            mac_from_mfr = mac_from_mfr_raw[::-1]
            print(f"    MAC in manufacturer data:   {_mac_str(mac_from_mfr)}")
            match = "✓ match" if mac_from_mfr.upper() == device.address.upper() else "✗ mismatch"
            print(f"    Match with BLE address:     {match}")
        else:
            print("    MAC in manufacturer data:   <payload too short>")

    if adv.rssi is not None:
        print(f"    RSSI:                       {adv.rssi} dBm")
    if adv.service_uuids:
        print(f"    Service UUIDs:              {adv.service_uuids}")
    print()


async def main():
    print("Scanning for GAN smart cubes (press Ctrl+C to stop)...\n")

    scanner = BleakScanner(detection_callback=_on_advertisement)

    try:
        await scanner.start()
        await asyncio.Event().wait()
    except asyncio.CancelledError:
        pass
    finally:
        await scanner.stop()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nStopped.")
        sys.exit(0)
