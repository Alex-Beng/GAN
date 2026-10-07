#!/usr/bin/env python3
"""Automated protocol test harness for GAN (ESP32-C3 over USB).

Pipeline:
  1. cargo build
  2. espflash flash --monitor  (serial log tee'd to scripts/logs/run_*.log)
  3. device IP parsed from boot logs ("IP=x.x.x.x") or supplied via --ip
  4. per-protocol HTTP matrix: switch / reset / single moves / state /
     seeded scramble + inverse / invalid input
  5. serial log scanned per test window for panics / reboots

Usage:
  python scripts/proto_test.py
  python scripts/proto_test.py --port COM7 --ip 192.168.50.246
  python scripts/proto_test.py --protocols moyu32,qiyi --quick
  python scripts/proto_test.py --skip-devices        # cube protocols only
"""
import argparse
import random
import re
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path

import requests

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cube_sim

ROOT = Path(__file__).resolve().parent.parent
LOGDIR = ROOT / "scripts" / "logs"
TARGET_BIN_DIR = ROOT / "target" / "riscv32imc-esp-espidf" / "debug"

PROTOS = {
    "gen2": 0, "gen3": 1, "gen4": 2, "gan251": 3,
    "moyu32": 4, "qiyi": 5, "robot": 6, "timer": 7,
}
PROTO_DEBUG = {0: "Gen2", 1: "Gen3", 2: "Gen4", 3: "Gan251",
               4: "MoYu32", 5: "QiYi", 6: "Robot", 7: "Timer"}
CUBE_PROTOCOLS = [0, 1, 2, 3, 4, 5]
DEVICE_PROTOCOLS = [6, 7]

# Gan251 firmware ignores faces D/L/B (only RUF supported), see ble.rs
GAN251_MAX_FACE = 2

PANIC_MARKERS = [
    b"panicked at",
    b"Backtrace",
    b"abort() was called",
    b"Guru Meditation",
    b"rst:0x",
    b"assert failed",
]


# ---------------------------------------------------------------- log capture

class LogCapture:
    """Background reader that tees a subprocess' stdout to a file and keeps
    the raw bytes in memory for offset-based window searches."""

    def __init__(self, proc, logfile):
        self.proc = proc
        self.buf = bytearray()
        self.lock = threading.Lock()
        self._f = open(logfile, "wb")
        self.thread = threading.Thread(target=self._pump, daemon=True)
        self.thread.start()

    def _pump(self):
        try:
            while True:
                chunk = self.proc.stdout.read1(4096)
                if not chunk:
                    break
                with self.lock:
                    self.buf += chunk
                self._f.write(chunk)
                self._f.flush()
        except Exception:
            pass
        finally:
            try:
                self._f.close()
            except Exception:
                pass

    def offset(self):
        with self.lock:
            return len(self.buf)

    def text(self, start=0, end=None):
        with self.lock:
            return bytes(self.buf[start:end]).decode("utf-8", "replace")

    def wait_pattern(self, pattern, timeout, start=0):
        pat = pattern.encode() if isinstance(pattern, str) else pattern
        deadline = time.time() + timeout
        while time.time() < deadline:
            with self.lock:
                idx = self.buf.find(pat, start)
            if idx >= 0:
                return idx
            time.sleep(0.2)
        return -1

    def find_from(self, pattern, start):
        pat = pattern.encode() if isinstance(pattern, str) else pattern
        with self.lock:
            return self.buf.find(pat, start)

    def scan_markers(self, start):
        with self.lock:
            data = bytes(self.buf[start:])
        hits = []
        for m in PANIC_MARKERS:
            i = data.find(m)
            if i >= 0:
                hits.append((m.decode(), start + i))
        return hits

    def excerpt(self, pos, before=300, after=900):
        with self.lock:
            lo = max(0, pos - before)
            hi = min(len(self.buf), pos + after)
        return bytes(self.buf[lo:hi]).decode("utf-8", "replace")


# ---------------------------------------------------------------- helpers

def detect_port():
    """Pick highest-numbered COM port (AGENTS.md: ESP32-C3 exposes two)."""
    try:
        from serial.tools import list_ports
    except ImportError:
        return None
    ports = []
    for p in list_ports.comports():
        m = re.fullmatch(r"COM(\d+)", p.device)
        if m:
            ports.append((int(m.group(1)), p.device))
    if not ports:
        return None
    ports.sort()
    return ports[-1][1]


def cargo_build():
    print("== cargo build ==  (emu crate, from emu/ for its sdkconfig)")
    r = subprocess.run(["cargo", "build", "-p", "emu"], cwd=str(ROOT / "emu"))
    if r.returncode != 0:
        sys.exit("FATAL: cargo build failed")
    for name in ("gan-emu", "gan-emu.exe"):
        p = TARGET_BIN_DIR / name
        if p.exists():
            return p
    sys.exit("FATAL: firmware binary not found under %s" % TARGET_BIN_DIR)


def espflash_prefix():
    if shutil.which("espflash"):
        return ["espflash"]
    return ["cargo", "espflash"]


def start_monitor(port, binpath, logfile):
    cmd = espflash_prefix() + [
        "flash", "--monitor", "--port", port, "--chip", "esp32c3", str(binpath),
    ]
    print("== %s ==" % " ".join(cmd))
    try:
        proc = subprocess.Popen(cmd, cwd=str(ROOT),
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    except FileNotFoundError as e:
        sys.exit("FATAL: cannot start espflash: %s" % e)
    cap = LogCapture(proc, logfile)
    return proc, cap


def discover_ip(cap, timeout=90):
    """Poll the log for a usable STA IP (skip DHCP-pending 0.0.0.0)."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        for m in re.finditer(r"IP=(?:Ok\()?(\d{1,3}(?:\.\d{1,3}){3})", cap.text()):
            if m.group(1) != "0.0.0.0":
                return m.group(1)
        time.sleep(0.5)
    return None


def wait_http_ready(base, timeout=30):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            r = requests.get(base + "/api/protocol", timeout=3)
            if r.status_code == 200:
                return True
        except requests.RequestException:
            pass
        time.sleep(1)
    return False


def http_get(base, path, timeout=5):
    return requests.get(base + path, timeout=timeout)


def http_post(base, path, payload=None, timeout=5):
    return requests.post(base + path, json=payload, timeout=timeout)


def wait_facelets(base, expected, timeout=3.0, poll=0.05):
    """Moves are applied asynchronously by the BLE loop; poll until match."""
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = http_get(base, "/api/facelets").text.strip()
        except requests.RequestException as e:
            last = "<http error: %s>" % e
        if last == expected:
            return True, last
        time.sleep(poll)
    return False, last


# ---------------------------------------------------------------- results

class AbortRun(Exception):
    pass


class ProtoResult:
    def __init__(self, name):
        self.name = name
        self.checks = 0
        self.failures = []

    def check(self, ok, label, detail=""):
        self.checks += 1
        if not ok:
            self.failures.append((label, str(detail)[:500]))
        return ok


# ---------------------------------------------------------------- protocol tests

def switch_protocol(ctx, cap, idx, res):
    """Switch protocol. The device reboots itself for the switch (by design),
    so wait for the fresh 'Starting BLE session' marker, re-discover the IP
    from the new boot logs (it can change, e.g. DHCP), and wait for HTTP.
    Returns the log offset just after the marker (for panic-window scans),
    or None if the switch failed."""
    base = ctx["base"]
    req_off = cap.offset()
    marker = ("Starting BLE session: " + PROTO_DEBUG[idx]).encode()
    # If the device already runs the target protocol there is no restart and
    # therefore no fresh marker -- just continue on the running session.
    try:
        if http_get(base, "/api/protocol").json().get("index") == idx:
            res.check(True, "T1.switch", "(already active)")
            return cap.offset()
    except (requests.RequestException, ValueError):
        pass
    try:
        r = http_post(base, "/api/protocol", {"index": idx}, timeout=8)
        res.check(r.status_code == 200 and PROTO_DEBUG[idx] in r.text,
                  "T1.switch", "resp %d %r" % (r.status_code, r.text))
    except requests.RequestException as e:
        res.check(False, "T1.switch", "http error: %s" % e)
    found = cap.wait_pattern(marker, 90, req_off)
    if found < 0:
        for m in (b"Guru Meditation", b"panicked at", b"abort() was called"):
            i = cap.find_from(m, req_off)
            if i >= 0:
                res.check(False, "T1.crash-during-switch", cap.excerpt(i))
                break
        res.check(False, "T1.session-timeout",
                  cap.text(req_off, req_off + 600))
        return None
    # Re-discover the IP from the new boot: STA may get a different lease or,
    # on weak signal, fall back to AP mode (unreachable from the LAN).
    boot_text = cap.text(req_off, found + len(marker))
    ips = [ip for ip in re.findall(
        r"IP=(?:Ok\()?(\d{1,3}(?:\.\d{1,3}){3})", boot_text) if ip != "0.0.0.0"]
    if not ips:
        res.check(False, "T1.wifi", boot_text[-400:])
        if "WiFi AP started" in boot_text:
            raise AbortRun(
                "device fell back to AP mode (no known network found after "
                "reboot) and is unreachable from this PC; check signal and rerun")
        return None
    if ips[-1] not in base:
        print("   (device IP changed: %s)" % ips[-1])
        ctx["base"] = base = "http://%s" % ips[-1]
    # httpd starts just before the BLE session log; ensure it answers
    if not wait_http_ready(base, timeout=30):
        res.check(False, "T1.http-ready", "no response after switch")
        return None
    g = http_get(base, "/api/protocol")
    if not res.check(g.status_code == 200 and ('"index":%d' % idx) in g.text,
                     "T1.confirm", g.text):
        return None
    return found + len(marker)


def scan_panics(cap, start, res):
    for marker, pos in cap.scan_markers(start):
        res.check(False, "T7.log-" + marker, cap.excerpt(pos))


def run_cube_protocol(ctx, cap, idx, quick=False):
    name = PROTO_DEBUG[idx]
    res = ProtoResult(name)
    start_off = cap.offset()
    print("\n---- %s ----" % name)

    scan_from = switch_protocol(ctx, cap, idx, res)
    if scan_from is None:
        scan_panics(cap, start_off, res)
        return res

    base = ctx["base"]

    # T2: reset -> solved
    http_post(base, "/api/moves/clear")
    http_post(base, "/api/reset")
    ok, got = wait_facelets(base, cube_sim.SOLVED_FACELETS)
    res.check(ok, "T2.reset", "got %s" % got)
    sim = cube_sim.CubeState()

    # T3: single moves, both JSON forms, facelets + /api/state
    max_face = GAN251_MAX_FACE if idx == 3 else 5
    faces = [1] if quick else range(max_face + 1)
    for face in faces:
        for d in (0, 1, 2):
            use_notation = (face + d) % 2 == 0
            mv = cube_sim.notation(face, d)
            if use_notation:
                r = http_post(base, "/api/move", {"move": mv})
            else:
                r = http_post(base, "/api/move", {"face": face, "dir": d})
            res.check(r.text.strip() == mv, "T3.resp-%s" % mv,
                      "resp %r" % r.text)
            cube_sim.apply_face_dir(sim, face, d)
            ok, got = wait_facelets(base, cube_sim.to_facelet_string(sim))
            res.check(ok, "T3.facelets-%s" % mv, "got %s" % got)
    s = http_get(base, "/api/state").json()
    exp = cube_sim.state_dict(sim)
    res.check(all(s[k] == exp[k] for k in exp), "T3.state", "%s != %s" % (s, exp))

    # T4/T5: seeded scramble + inverse (start from solved so the inverse
    # round-trip must return to solved)
    if not quick:
        http_post(base, "/api/reset")
        sim = cube_sim.CubeState()
        n_faces = GAN251_MAX_FACE + 1 if idx == 3 else 6
        rng = random.Random(42 + idx)
        scramble, last_face = [], -1
        while len(scramble) < 15:
            f = rng.randrange(n_faces)
            if f == last_face:
                continue
            last_face = f
            scramble.append((f, rng.randrange(3)))

        items, tokens = [], []
        for i, (f, d) in enumerate(scramble):
            mv = cube_sim.notation(f, d)
            items.append({"move": mv})
            tokens.append(mv)
            if i == 4:
                items.append({"delay": 20}); tokens.append("delay=20")
            if i == 9:
                items.append({"time": 100}); tokens.append("time=100")

        r = http_post(base, "/api/moves", items, timeout=10)
        res.check(r.text.strip().split(",") == tokens, "T4.batch-resp",
                  "resp %r" % r.text)
        for f, d in scramble:
            cube_sim.apply_face_dir(sim, f, d)
        ok, got = wait_facelets(base, cube_sim.to_facelet_string(sim), timeout=8)
        res.check(ok, "T4.scramble", "got %s" % got)

        inv = [(f, {0: 1, 1: 0, 2: 2}[d]) for f, d in reversed(scramble)]
        r = http_post(base, "/api/moves",
                      [{"move": cube_sim.notation(f, d)} for f, d in inv], timeout=10)
        ok, got = wait_facelets(base, cube_sim.SOLVED_FACELETS, timeout=8)
        res.check(ok, "T5.inverse", "got %s" % got)

    # T6: invalid input must not crash the device
    for payload, want in (
        ({"face": 9, "dir": 0}, "invalid"),
        ({"move": "X"}, "bad request"),
        ({"face": 0}, "bad request"),
    ):
        r = http_post(base, "/api/move", payload)
        res.check(r.text.strip() == want, "T6.%s" % want,
                  "payload %r -> %r" % (payload, r.text))
    r = http_post(base, "/api/moves", [{"junk": 1}])
    res.check(r.text.strip() == "bad request", "T6.batch", r.text)
    r = http_get(base, "/api/protocol")
    res.check(r.status_code == 200, "T6.alive", r.text)

    scan_panics(cap, scan_from, res)
    return res


def run_device_protocol(ctx, cap, idx):
    name = PROTO_DEBUG[idx]
    res = ProtoResult(name)
    start_off = cap.offset()
    print("\n---- %s (smoke) ----" % name)

    scan_from = switch_protocol(ctx, cap, idx, res)
    if scan_from is None:
        scan_panics(cap, start_off, res)
        return res

    base = ctx["base"]

    # GATT config endpoint reflects the device profile
    g = http_get(base, "/api/gatt")
    want_device = "GAN-ROBOT" if idx == 6 else "GAN-TIMER"
    want_mac = ":D0" if idx == 6 else ":D1"
    res.check(want_device in g.text and want_mac in g.text, "S.gatt", g.text)

    # Moves are accepted but not applied (queue skipped for Robot/Timer)
    r = http_post(base, "/api/move", {"move": "R"})
    res.check(r.text.strip() == "R", "S.move-accepted", r.text)
    ok, got = wait_facelets(base, cube_sim.SOLVED_FACELETS)
    res.check(ok, "S.move-noop", "got %s" % got)
    http_post(base, "/api/moves/clear")  # keep queue empty for next protocol

    g = http_get(base, "/api/protocol")
    res.check(g.status_code == 200, "S.alive", g.text)

    scan_panics(cap, scan_from, res)
    return res


# ---------------------------------------------------------------- main

def print_report(results):
    print("\n" + "=" * 62)
    print("RESULTS")
    print("=" * 62)
    failed = 0
    for r in results:
        status = "PASS" if not r.failures else "FAIL"
        if r.failures:
            failed += 1
        print("%-8s: %s  (%d checks)" % (r.name, status, r.checks))
        for label, detail in r.failures:
            print("    [FAIL] %s" % label)
            for line in detail.splitlines()[:12]:
                print("           %s" % line)
    total = len(results)
    print("-" * 62)
    print("%d/%d protocols passed" % (total - failed, total))
    return failed == 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", help="COM port for espflash (default: highest COMx)")
    ap.add_argument("--ip", help="device IP (default: parse from boot logs)")
    ap.add_argument("--protocols", help="comma list, e.g. gen2,moyu32 (default: all)")
    ap.add_argument("--quick", action="store_true", help="single-face smoke")
    ap.add_argument("--skip-devices", action="store_true",
                    help="skip robot/timer smoke tests")
    args = ap.parse_args()

    port = args.port or detect_port()
    if not port:
        sys.exit("FATAL: no COM port detected; pass --port COMx")

    selected = list(CUBE_PROTOCOLS)
    if not args.skip_devices:
        selected += DEVICE_PROTOCOLS
    if args.protocols:
        try:
            selected = [PROTOS[p.strip().lower()] for p in args.protocols.split(",")]
        except KeyError as e:
            sys.exit("FATAL: unknown protocol %s (valid: %s)" % (e, ",".join(PROTOS)))

    binpath = cargo_build()

    LOGDIR.mkdir(parents=True, exist_ok=True)
    logfile = LOGDIR / ("run_%s.log" % datetime.now().strftime("%Y%m%d_%H%M%S"))
    print("== log: %s ==" % logfile)
    proc, cap = start_monitor(port, binpath, logfile)

    base = None
    orig = None
    ctx = {"base": None}
    results = []
    try:
        ip = args.ip or discover_ip(cap)
        if not ip:
            sys.exit("FATAL: no IP found in logs after 90s; connect device to "
                     "your LAN or pass --ip")
        print("== device IP: %s ==" % ip)
        base = "http://%s" % ip
        if not wait_http_ready(base):
            sys.exit("FATAL: HTTP not ready at %s" % base)

        orig = http_get(base, "/api/protocol").json().get("index")
        print("== original protocol index: %s ==" % orig)

        ctx["base"] = base
        aborted = None
        for idx in selected:
            if aborted:
                break
            try:
                if idx in CUBE_PROTOCOLS:
                    results.append(run_cube_protocol(ctx, cap, idx, args.quick))
                else:
                    results.append(run_device_protocol(ctx, cap, idx))
            except AbortRun as e:
                aborted = str(e)
                break
        if aborted:
            print("\nABORTED: %s" % aborted)
    finally:
        restore_base = ctx["base"] or base
        if restore_base:
            try:
                http_post(restore_base, "/api/protocol", {"index": orig})
                time.sleep(1.5)  # allow BLE restart before killing monitor
            except Exception:
                pass
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()

    ok = print_report(results)
    print("full log: %s" % logfile)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
