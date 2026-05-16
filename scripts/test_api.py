import requests
import sys
import json
import time

MODE = "http"
PORT = None
BASE = sys.argv[1] if len(sys.argv) > 1 else "http://192.168.50.246"
SERIAL_PORT = "COM3"

try:
    import serial
    HAS_SERIAL = True
except ImportError:
    HAS_SERIAL = False


def ser_readline(timeout=1.0):
    end = time.time() + timeout
    while time.time() < end:
        line = PORT.readline().decode(errors="ignore").strip()
        if line.startswith(">>> "):
            return line[4:]
    return None


def ser_open():
    global PORT
    if not HAS_SERIAL:
        print("pyserial not installed, run: pip install pyserial")
        return False
    if PORT is None:
        p = input(f"串口 ({SERIAL_PORT}): ").strip() or SERIAL_PORT
        PORT = serial.Serial(p, 115200, timeout=0.1)
        time.sleep(0.5)
        resp = ser_readline(2.0)
        if resp:
            print(resp)
    return True


def ser_close():
    global PORT
    if PORT:
        PORT.close()
        PORT = None
        print("串口已关闭")


def ser_send(cmd):
    if not ser_open():
        return
    PORT.write((cmd + "\n").encode())
    resp = ser_readline(1.0)
    if resp:
        print(f"-> {resp}")


def post(path, data):
    r = requests.post(f"{BASE}{path}", json=data)
    print(f"POST {path} {data} -> {r.status_code}: {r.text}")


def get(path):
    r = requests.get(f"{BASE}{path}")
    print(f"GET {path} -> {r.status_code}: {r.text}")


def test_move():
    for face, name in enumerate(["U", "R", "F", "D", "L", "B"]):
        post("/api/move", {"face": face, "dir": 0})
        post("/api/move", {"face": face, "dir": 1})


def test_move_serial():
    if not ser_open():
        return
    for face, name in enumerate(["U", "R", "F", "D", "L", "B"]):
        ser_send(json.dumps([{"face": face, "dir": 0}]))
        ser_send(json.dumps([{"face": face, "dir": 1}]))


def test_moves():
    moves = [{"face": 0, "dir": 0}, {"face": 1, "dir": 1}, {"face": 2, "dir": 0}]
    post("/api/moves", moves)


def test_moves_serial():
    moves = [{"face": 0, "dir": 0}, {"face": 1, "dir": 1}, {"face": 2, "dir": 0}]
    ser_send(json.dumps(moves))


def test_facelets():
    get("/api/facelets")


def test_facelets_serial():
    ser_send("facelets")


def test_wifi_ap():
    post("/api/wifi/mode", {"ap": 1})


def test_wifi_sta():
    ssid = input("STA SSID: ")
    pwd = input("STA Password: ")
    post("/api/wifi/mode", {"ap": 0, "ssid": ssid, "pass": pwd})


def toggle_mode():
    global MODE
    if MODE == "http":
        if not HAS_SERIAL or not ser_open():
            return
        MODE = "serial"
        print(f"切换到串口模式 ({PORT.port})")
    else:
        ser_close()
        MODE = "http"
        print(f"切换到 HTTP 模式 ({BASE})")


while True:
    print(f"\n=== GAN Cube Emu [{MODE.upper()}] ===")
    if MODE == "http":
        print("1.单步  2.批量  3.面片状态  4.切AP  5.切STA  6.切串口  q.退出")
        c = input("> ").strip()
        if c == "1": test_move()
        elif c == "2": test_moves()
        elif c == "3": test_facelets()
        elif c == "4": test_wifi_ap()
        elif c == "5": test_wifi_sta()
        elif c == "6": toggle_mode()
        elif c == "q": break
    else:
        print("1.单步  2.批量  3.面片状态  6.切HTTP  q.退出")
        c = input("> ").strip()
        if c == "1": test_move_serial()
        elif c == "2": test_moves_serial()
        elif c == "3": test_facelets_serial()
        elif c == "6": toggle_mode()
        elif c == "q":
            ser_close()
            break
