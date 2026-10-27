#!/usr/bin/env python3
"""V3 (Gen4 / Gan251) frame conformance check against the decrypted GAN SDK JSON.

The GAN SDK describes each BLE protocol as a JSON document (an encrypted
`assets/GanSDK_Protocol*.alpha` inside CubeStation.apk). This script compares
the emulator's frame layout against those definitions so the two stay in sync.

Decrypt the definitions first (they are not committed):

    python scripts/decrypt_gansdk_alpha.py <CubeStation.apk> <outdir>

Then run:

    python scripts/check_v3_frames.py <outdir>

What is verified:

  1. `isCRC16` -- ProtocolV3 and ProtocolV3-2 both set it, so every uplink frame
     must carry a trailing CRC-16/CCITT-FALSE, and the length byte counts it.
     (The app trims trailing zeros, then checks crc16(frame[..n-2]) against the
     last two bytes: BleModule -> h.GetCrc_16.)
  2. Uplink opcodes the emulator emits (1/209/236/237/239) exist in the JSON.
  3. Two V3-specific trailing fields: `offset:3` on 237 (V3 only, absent from
     V3-2) and `offset:4` on 236.
  4. Downlink protoIds from GanSDK_ProtocolWriteV3.json match what
     `parse_command_type` recognises.

Usage note: --strict turns a missing definition directory into an error instead
of a skip, useful in CI once the APK is available.
"""
import argparse
import json
import os
import sys

# --- emulator uplink opcodes (shared/src/protocol.rs, Gen4/Gan251 branches) ---
EMU_UPLINK = {
    1: "move (time:32 step:16 formula:2+6)",
    209: "move history (step:8 formulaHistory:3+1)",
    236: "gyro (dataFour1:1+15 x4, angleSpeed1:1+3 x3)",
    237: "facelets (step:16 cp:3x7 co:2x8 ep:4x11 eo:1x12)",
    239: "battery (index:8 batteryPower:8)",
}

# --- emulator downlink handlers (shared/src/protocol.rs parse_command_type) ---
EMU_DOWNLINK = {
    (0xDD, 0x04): "facelets / battery request (inner discriminator at data[3])",
    (0xDF, 0x03): "hardware info block",
    (0xD2, 0x0D): "reset to solved state",
    (0xD1, 0x04): "move history request",
    (0xD3, 0x01): "ack (appProto 13)",
    (0xD4, 0x01): "ack (appProto 15)",
    (0xF0, 0x01): "ack (appProto 16)",
    (0x11, 0x01): "colour calibration (appProto 21)",
    (0x04, 0x01): "model query (appProto 40)",
}

FAILURES = []


def check(ok, label, detail=""):
    print("  [%s] %s%s" % ("PASS" if ok else "FAIL", label,
                           (" | " + str(detail)) if detail and not ok else ""))
    if not ok:
        FAILURES.append((label, str(detail)))
    return ok


def crc16_ccitt_false(data):
    """Mirror of shared/src/crypto.rs::crc16_ccitt_false (poly 0x1021, init 0xFFFF)."""
    crc = 0xFFFF
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def seal_v3(buf, used):
    """Mirror of shared/src/protocol.rs::v3_seal."""
    ln = min(used + 2, 20)
    buf[1] = ln - 2
    crc = crc16_ccitt_false(bytes(buf[:ln - 2]))
    buf[ln - 2] = (crc >> 8) & 0xFF
    buf[ln - 1] = crc & 0xFF
    return bytes(buf[:ln])


def load(outdir, name):
    path = os.path.join(outdir, "GanSDK_Protocol%s.json" % name)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def fields_of(entry):
    out = []
    for f in entry.get("data", []):
        bits = [s.get("bitCount") for s in f.get("subElements", [])]
        out.append((f.get("keyName"), tuple(bits), f.get("elementCount")))
    return out


def check_uplink(name, doc):
    print("\n[%s] uplink" % name)
    by_id = {p["bleProtoId"]: p for p in doc["protocols"]}
    for op, desc in sorted(EMU_UPLINK.items()):
        check(op in by_id, "opcode %d defined (%s)" % (op, desc),
              "not in GanSDK_Protocol%s.json" % name)
    return by_id


def check_offsets(v3, v32):
    print("\n[V3 vs V3-2] trailing offset fields")
    v3_fields = {p["bleProtoId"]: fields_of(p) for p in v3["protocols"]}
    v32_fields = {p["bleProtoId"]: fields_of(p) for p in v32["protocols"]}

    v3_237 = [f[0] for f in v3_fields.get(237, [])]
    v32_237 = [f[0] for f in v32_fields.get(237, [])]
    check("offset" in v3_237, "V3 237 facelets carries offset:3", v3_237)
    check("offset" not in v32_237, "V3-2 237 facelets has no offset", v32_237)

    v3_236 = [f[0] for f in v3_fields.get(236, [])]
    check("offset" in v3_236, "V3 236 gyro carries offset:4", v3_236)


def check_downlink(write_doc):
    print("\n[WriteV3] downlink protoIds")
    seen = {}
    for entry in write_doc["protocols"]:
        proto_id = entry["data"][0]["defaultData"]
        first = proto_id[0]["values"][0]
        second = proto_id[1]["values"][0] if len(proto_id) > 1 else None
        seen[(first, second)] = entry.get("appProtoId")
    for key, desc in sorted(EMU_DOWNLINK.items()):
        check(key in seen, "downlink %02X %02X handled (%s)" % (key[0], key[1], desc),
              "not in GanSDK_ProtocolWriteV3.json")
    return seen


def check_crc_frames():
    print("\n[frames] CRC16 + length byte")
    # battery 239: EF <len> 00 50 <crc16>
    buf = bytearray(20)
    buf[0] = 0xEF
    buf[2], buf[3] = 0, 80
    frame = seal_v3(buf, 4)
    check(frame[1] == 4, "battery length byte counts 4 bytes after itself", frame[1])
    check(len(frame) == 6, "battery frame is 6 bytes", len(frame))
    check(frame[4:] == bytes([crc16_ccitt_false(frame[:4]) >> 8,
                               crc16_ccitt_false(frame[:4]) & 0xFF]),
          "battery CRC16 big-endian over payload", frame.hex())

    # facelets 237 (V3): ED <len> serial:2 state:12 offset:1 -> 16 + crc
    buf = bytearray(20)
    buf[0] = 0xED
    buf[3] = 1
    frame = seal_v3(buf, 17)
    check(frame[1] == 17, "V3 facelets length byte counts 17 bytes", frame[1])
    check(len(frame) == 19, "V3 facelets frame is 19 bytes", len(frame))

    # facelets 237 (V3-2): no trailing offset -> 15 + crc
    buf = bytearray(20)
    buf[0] = 0xED
    buf[3] = 1
    frame = seal_v3(buf, 16)
    check(frame[1] == 16, "V3-2 facelets length byte counts 16 bytes", frame[1])
    check(len(frame) == 18, "V3-2 facelets frame is 18 bytes", len(frame))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("outdir", help="directory with decrypted GanSDK_Protocol*.json")
    ap.add_argument("--strict", action="store_true",
                    help="fail (instead of skip) when definitions are missing")
    args = ap.parse_args()

    v3 = load(args.outdir, "V3")
    v32 = load(args.outdir, "V3-2")
    write = load(args.outdir, "WriteV3")
    if not (v3 and v32 and write):
        msg = "missing decrypted definitions in %s (run decrypt_gansdk_alpha.py)" % args.outdir
        if args.strict:
            print("FAIL: " + msg)
            return 1
        print("SKIP: " + msg)
        check_crc_frames()
        return 0 if not FAILURES else 1

    check(v3.get("isCRC16") == 1, "V3 isCRC16 == 1")
    check(v32.get("isCRC16") == 1, "V3-2 isCRC16 == 1")
    check(v3.get("servicesUUID", "").lower() == "00000010-0000-fff7-fff6-fff5fff4fff0",
          "V3 service UUID", v3.get("servicesUUID"))
    check(v32.get("secretKey") and v32.get("secretIvKey"),
          "V3-2 has its own secretKey/secretIvKey")

    check_uplink("V3", v3)
    check_uplink("V3-2", v32)
    check_offsets(v3, v32)
    check_downlink(write)
    check_crc_frames()

    print()
    if FAILURES:
        print("%d failure(s)" % len(FAILURES))
        return 1
    print("all V3 frame checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
