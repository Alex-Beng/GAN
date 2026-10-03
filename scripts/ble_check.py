#!/usr/bin/env python3
"""BLE conformance check for the MoYu32 (WCU) emulator, using bleak.

Validates the emulator against the public WCU reference implementation
(csTimer src/js/hardware/moyu32cube.js):

  1. Advertising: name pattern, advertised service UUID, manufacturer data
     (CIC 0x0000 + reversed MAC in the last 6 bytes)
  2. GATT: connect, derive keys from the advertised MAC, send encrypted
     requests 161 (info) / 163 (state) / 164 (battery)
  3. Decode 163 facelets and 165 move notifications per the WCU bit layout
     and compare against the offline cube_sim mirror (moves injected via HTTP)

Usage:
  python scripts/ble_check.py [--ip 192.168.50.246] [--no-switch]

Requires: pip install bleak pycryptodome requests
"""
import argparse
import asyncio
import sys
import time
from pathlib import Path

import requests
from bleak import BleakScanner, BleakClient
from Crypto.Cipher import AES

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cube_sim

SERVICE_UUID = "0783b03e-7735-b5a0-1760-a305d2795cb0"
CHR_READ_UUID = "0783b03e-7735-b5a0-1760-a305d2795cb1"   # notify
CHR_WRITE_UUID = "0783b03e-7735-b5a0-1760-a305d2795cb2"  # write

EXPECT_NAME = "WCU_MY32_A4CE"
EXPECT_MAC = [0xCF, 0x30, 0x16, 0x00, 0xA4, 0xCE]

# Base key/IV (verified identical to csTimer moyu32cube.js KEYS)
BASE_KEY = [21, 119, 58, 92, 103, 14, 45, 31, 23, 103, 42, 19, 155, 103, 82, 87]
BASE_IV = [17, 35, 38, 37, 134, 42, 44, 59, 85, 6, 127, 49, 126, 103, 33, 87]

URFDLB_TO_FBUDLR = [2, 5, 0, 3, 4, 1]  # U R F D L B -> F B U D L R
FBUDLR = "FBUDLR"


class Check:
    def __init__(self):
        self.failures = []
        self.count = 0

    def check(self, ok, label, detail=""):
        self.count += 1
        status = "PASS" if ok else "FAIL"
        print("  [%s] %s%s" % (status, label, (" | " + str(detail)) if detail and not ok else ""))
        if not ok:
            self.failures.append((label, str(detail)[:300]))
        return ok


# ---------------------------------------------------------------- WCU crypto

def derive_key_iv(mac_bytes):
    salt = list(reversed(mac_bytes))
    key, iv = list(BASE_KEY), list(BASE_IV)
    for i in range(6):
        key[i] = (key[i] + salt[i]) % 255
        iv[i] = (iv[i] + salt[i]) % 255
    return key, iv


def ecb(data, key, encrypt):
    aes = AES.new(bytes(key), AES.MODE_ECB)
    return aes.encrypt(bytes(data)) if encrypt else aes.decrypt(bytes(data))


def wcu_encrypt(data, key, iv):
    buf = bytearray(data)
    for i in range(16):
        buf[i] ^= iv[i]
    buf[0:16] = ecb(buf[0:16], key, True)
    if len(buf) > 16:
        off = len(buf) - 16
        for i in range(16):
            buf[off + i] ^= iv[i]
        buf[off:off + 16] = ecb(buf[off:off + 16], key, True)
    return buf


def wcu_decrypt(data, key, iv):
    buf = bytearray(data)
    if len(buf) > 16:
        off = len(buf) - 16
        buf[off:off + 16] = ecb(buf[off:off + 16], key, False)
        for i in range(16):
            buf[off + i] ^= iv[i]
    buf[0:16] = ecb(buf[0:16], key, False)
    for i in range(16):
        buf[i] ^= iv[i]
    return buf


# ---------------------------------------------------------------- decoders

def bits_of(data):
    return "".join("{:08b}".format(b) for b in data)


def parse_type(plain):
    return int(bits_of(plain)[0:8], 2)


def parse_facelet_msg(plain):
    """Returns (moveCnt, facelet_string in URFDLB letters)."""
    bits = bits_of(plain)
    move_cnt = int(bits[152:160], 2)
    fb = bits[8:152]
    out = []
    for i in range(6):  # output faces in URFDLB order
        face = fb[URFDLB_TO_FBUDLR[i] * 24: URFDLB_TO_FBUDLR[i] * 24 + 24]
        for j in range(8):
            out.append(FBUDLR[int(face[j * 3:j * 3 + 3], 2)])
            if j == 3:  # center is implied by the storage face
                out.append(FBUDLR[URFDLB_TO_FBUDLR[i]])
    return move_cnt, "".join(out)


def parse_move_msg(plain):
    """Returns (moveCnt, moves[5], times[5]); None marks an empty slot."""
    bits = bits_of(plain)
    times = [int(bits[8 + i * 16:24 + i * 16], 2) for i in range(5)]
    move_cnt = int(bits[88:96], 2)
    moves = []
    for i in range(5):
        m = int(bits[96 + i * 5:101 + i * 5], 2)
        moves.append(None if m >= 12 else FBUDLR[m >> 1] + ("'" if m & 1 else ""))
    return move_cnt, moves, times


def parse_hardware_msg(plain):
    bits = bits_of(plain)
    name = "".join(chr(int(bits[8 + i * 8:16 + i * 8], 2)) for i in range(8))
    hw = "%d.%d" % (int(bits[72:80], 2), int(bits[80:88], 2))
    sw = "%d.%d" % (int(bits[88:96], 2), int(bits[96:104], 2))
    return name, hw, sw


# ---------------------------------------------------------------- helpers

def http(base, method, path, payload=None, timeout=5):
    if method == "get":
        return requests.get(base + path, timeout=timeout)
    return requests.post(base + path, json=payload, timeout=timeout)


def wait_protocol(base, idx, timeout=90):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            r = requests.get(base + "/api/protocol", timeout=3)
            if r.status_code == 200 and ('"index":%d' % idx) in r.text:
                return True
        except requests.RequestException:
            pass
        time.sleep(1)
    return False


async def wait_notification(queue, want_type, key, iv, timeout=4.0):
    """Consume notifications until one decrypts to the wanted message type."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            data = await asyncio.wait_for(queue.get(), timeout=max(0.1, deadline - time.time()))
        except asyncio.TimeoutError:
            break
        try:
            plain = wcu_decrypt(bytearray(data), key, iv)
        except Exception as e:
            print("  (decrypt failed: %s)" % e)
            continue
        if parse_type(plain) == want_type:
            return plain
    return None


# ---------------------------------------------------------------- main

async def run(base, switch, ck):
    print("== 1. HTTP: ensure MoYu32 protocol, reset state ==")
    if switch:
        cur = http(base, "get", "/api/protocol").json().get("index")
        if cur != 4:
            http(base, "post", "/api/protocol", {"index": 4})
            if not ck.check(wait_protocol(base, 4), "protocol switch to MoYu32"):
                return
        else:
            print("   (already MoYu32)")
    else:
        ck.check(http(base, "get", "/api/protocol").json().get("index") == 4,
                 "device already on MoYu32")
    http(base, "post", "/api/moves/clear")
    http(base, "post", "/api/reset")
    time.sleep(0.3)

    print("== 2. BLE scan: advertising identity ==")
    devices = await BleakScanner.discover(timeout=8.0, return_adv=True)
    target = None
    for addr, (dev, adv) in devices.items():
        if adv.local_name and adv.local_name.startswith("WCU_MY3"):
            target = (dev, adv)
            break
    if not ck.check(target is not None, "scan finds WCU_MY3* device",
                    "names seen: %s" % [a.local_name for _, a in devices.values() if a.local_name]):
        return
    dev, adv = target
    ck.check(adv.local_name == EXPECT_NAME, "device name", adv.local_name)
    ck.check(SERVICE_UUID in [u.lower() for u in adv.service_uuids],
             "adv contains service UUID", adv.service_uuids)
    mfr = adv.manufacturer_data or {}
    cic, payload = next(iter(mfr.items())) if mfr else (None, None)
    ck.check(cic == 0x0000, "manufacturer CIC == 0x0000", hex(cic) if cic is not None else "none")
    if payload:
        # WCU app matches packetData[4] against its ProtocolFlag table
        # (BluetoothUtil.CheckProtocolVersion). 0x30 selects CubeProtocol5,
        # which is the wire format this emulator implements.
        ck.check(len(payload) > 4 and payload[4] == 0x30,
                 "mfr data byte[4] == 0x30 (CubeProtocol5 flag)",
                 "got %s" % (hex(payload[4]) if len(payload) > 4 else "short"))
        mac_recv = list(payload[-6:])[::-1]
        ck.check(mac_recv == EXPECT_MAC, "mfr data last-6 reversed == MAC",
                 "got %s" % ":".join("%02X" % b for b in mac_recv))

    print("== 3. GATT connect: info/state/battery requests ==")
    queue = asyncio.Queue()

    def on_notify(_handle, data):
        queue.put_nowait(bytes(data))

    async with BleakClient(dev.address, disconnected_callback=None) as client:
        ck.check(client.is_connected, "connected")
        svcs = [str(s.uuid).lower() for s in client.services]
        ck.check(SERVICE_UUID in svcs, "GATT service present", svcs)
        chars = [str(c.uuid).lower() for s in client.services for c in s.characteristics]
        ck.check(CHR_READ_UUID in chars and CHR_WRITE_UUID in chars,
                 "read/write characteristics present")

        mac_bytes = EXPECT_MAC  # validated against adv above
        key, iv = derive_key_iv(mac_bytes)
        await client.start_notify(CHR_READ_UUID, on_notify)
        time.sleep(0.2)  # let the subscribe-time initial facelets arrive

        def send_request(opcode):
            req = bytearray(20)
            req[0] = opcode
            return client.write_gatt_char(CHR_WRITE_UUID, bytes(wcu_encrypt(req, key, iv)))

        # 161 hardware info
        await send_request(161)
        plain = await wait_notification(queue, 161, key, iv)
        if ck.check(plain is not None, "161 info reply"):
            name, hw, sw = parse_hardware_msg(plain)
            ck.check(name == "WCU_MY32", "161 device name", name)
            ck.check(hw == "1.0", "161 hw version", hw)
            ck.check(sw == "2.0", "161 sw version", sw)

        # 163 initial state (solved after reset)
        await send_request(163)
        plain = await wait_notification(queue, 163, key, iv)
        if ck.check(plain is not None, "163 state reply"):
            move_cnt, facelets = parse_facelet_msg(plain)
            ck.check(facelets == cube_sim.SOLVED_FACELETS, "163 solved facelets",
                     facelets)
            serial = move_cnt
        else:
            serial = 0

        # 164 battery
        await send_request(164)
        plain = await wait_notification(queue, 164, key, iv)
        if ck.check(plain is not None, "164 battery reply"):
            level = int(bits_of(plain)[8:16], 2)
            ck.check(0 <= level <= 100, "164 battery level", level)

        print("== 4. Move notifications (165) ==")
        sim = cube_sim.CubeState()

        async def drain_queue():
            while not queue.empty():
                try:
                    queue.get_nowait()
                except asyncio.QueueEmpty:
                    break

        async def move_test(mv):
            nonlocal serial
            face, d = cube_sim.parse_notation(mv)
            exp_face = FBUDLR[URFDLB_TO_FBUDLR[face]]
            # one 165 notification per queued entry: X2 pushes two CW quarters
            expected = [exp_face + ("'" if d == 1 else "")] if d != 2 else [exp_face] * 2
            http(base, "post", "/api/move", {"move": mv})
            got = []
            for _ in expected:
                plain = await wait_notification(queue, 165, key, iv, timeout=3.0)
                if plain is None:
                    ck.check(False, "165 notification for %s" % mv)
                    return
                mc, moves, times = parse_move_msg(plain)
                got.append(moves[0])
                ck.check(mc == (serial + len(got)) % 256,
                         "165 moveCnt %d -> %d" % (serial, mc), mc)
            ck.check(got == expected, "165 slot0 moves == %s" % mv, got)
            serial = (serial + len(expected)) % 256
            cube_sim.apply_face_dir(sim, face, d)
            # discard the intermediate per-quarter 163 pushes, then ask for state
            await drain_queue()
            await send_request(163)
            plain = await wait_notification(queue, 163, key, iv)
            if ck.check(plain is not None, "163 after %s" % mv):
                _, facelets = parse_facelet_msg(plain)
                ck.check(facelets == cube_sim.to_facelet_string(sim),
                         "163 facelets match sim after %s" % mv, facelets)

        for mv in ["R", "U'", "F2", "L", "D'", "B2"]:
            await move_test(mv)

        print("== 5. Control opcodes (170/172/173/174/168) ==")

        def wbits(buf, bitpos, value, count):
            for i in range(count):
                bit = (value >> (count - 1 - i)) & 1
                idx = bitpos + i
                if bit:
                    buf[idx // 8] |= 1 << (7 - (idx % 8))

        async def send_plain(plain):
            enc = wcu_encrypt(bytearray(plain), key, iv)
            await client.write_gatt_char(CHR_WRITE_UUID, bytes(enc))

        async def control_test(label, plain, want_type, timeout=3.0):
            await drain_queue()
            await send_plain(plain)
            return await wait_notification(queue, want_type, key, iv, timeout=timeout)

        # --- 170 bind account ---
        acct = 0x12345678
        req = bytearray(20)
        req[0] = 170
        wbits(req, 8, acct, 32)
        plain = await control_test("bind", req, 170)
        if ck.check(plain is not None, "170 bind reply"):
            got_acct = int(bits_of(plain)[8:40], 2)
            ck.check(got_acct == acct, "170 echoes account", hex(got_acct))

        # Device re-advertises with the account in mfr data bytes[0..4); rescan.
        await asyncio.sleep(6.0)
        devs = await BleakScanner.discover(timeout=8.0, return_adv=True)
        found = None
        for _addr, (d2, a2) in devs.items():
            if a2.local_name and a2.local_name.startswith("WCU_MY3"):
                found = (d2, a2)
                break
        if ck.check(found is not None, "cube re-advertised after bind"):
            _d2, a2 = found
            mfr2 = a2.manufacturer_data or {}
            _cic2, payload2 = next(iter(mfr2.items())) if mfr2 else (None, None)
            if payload2:
                adv_acct = int.from_bytes(bytes(payload2[0:4]), "big")
                ck.check(adv_acct == acct, "adv account == bound account", hex(adv_acct))
                ck.check(len(payload2) > 4 and payload2[4] == 0x30,
                         "adv byte[4] still 0x30", hex(payload2[4]) if len(payload2) > 4 else "short")
            # reconnect for the remaining tests
            async with BleakClient(_d2.address) as client2:
                ck.check(client2.is_connected, "reconnect after bind")
                await client2.start_notify(CHR_READ_UUID, on_notify)
                # rebind sender context to the new client
                async def send_plain2(p):
                    await client2.write_gatt_char(CHR_WRITE_UUID, bytes(wcu_encrypt(bytearray(p), key, iv)))
                # --- 172 gyro op ---
                req = bytearray(20)
                req[0] = 172
                req[1] = 1
                req[2] = 1
                await drain_queue()
                await send_plain2(req)
                plain = await wait_notification(queue, 172, key, iv)
                if ck.check(plain is not None, "172 gyro op reply"):
                    b = bits_of(plain)
                    ck.check(int(b[8:16], 2) == 1 and int(b[16:24], 2) == 1,
                             "172 echoes (reset=1,open=1)")
                # --- 173 change name ---
                req = bytearray(20)
                req[0] = 173
                nm = b"WCU_MY37_TEST"
                req[1] = len(nm)
                for i, ch in enumerate(nm):
                    wbits(req, 16 + i * 8, ch, 8)
                await drain_queue()
                await send_plain2(req)
                plain = await wait_notification(queue, 173, key, iv)
                ck.check(plain is not None, "173 change name reply")
                # --- 174 solving (echo state) ---
                req = bytearray(20)
                req[0] = 174
                for i in range(48):
                    wbits(req, 8 + i * 3, i % 6, 3)
                await drain_queue()
                await send_plain2(req)
                plain = await wait_notification(queue, 174, key, iv)
                ck.check(plain is not None, "174 solving reply")
                # --- 168 move history ---
                req = bytearray(20)
                req[0] = 168
                req[1] = 0
                await drain_queue()
                await send_plain2(req)
                plain = await wait_notification(queue, 168, key, iv)
                ck.check(plain is not None, "168 move history reply")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--ip", default="192.168.50.246", help="device IP")
    ap.add_argument("--no-switch", action="store_true",
                    help="do not switch the device to MoYu32 first")
    args = ap.parse_args()

    base = "http://%s" % args.ip
    ck = Check()
    asyncio.run(run(base, not args.no_switch, ck))

    print("\n" + "=" * 50)
    if ck.failures:
        print("FAILED: %d/%d checks" % (len(ck.failures), ck.count))
        for label, detail in ck.failures:
            print("  [FAIL] %s: %s" % (label, detail))
        sys.exit(1)
    print("PASSED: %d/%d checks" % (ck.count, ck.count))


if __name__ == "__main__":
    main()
