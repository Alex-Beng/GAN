import requests
import sys
import msvcrt

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://192.168.50.246"

FACE_MAP = {
    'u': 0, 'r': 1, 'f': 2, 'd': 3, 'l': 4, 'b': 5,
    'U': 0, 'R': 1, 'F': 2, 'D': 3, 'L': 4, 'B': 5,
}

NAMES = ['U', 'R', 'F', 'D', 'L', 'B']

DELAY = 50


def send_move(face, dir):
    r = requests.post(f"{BASE}/api/move", json={"face": face, "dir": dir})
    label = NAMES[face] + ("'" if dir else "")
    print(f"  {label} -> {r.text.strip()}")


def set_delay(ms):
    global DELAY
    DELAY = max(10, min(2000, ms))
    r = requests.post(f"{BASE}/api/delay", json={"delay": DELAY})
    print(f"  delay={DELAY}ms -> {r.text.strip()}")


def test_inline_delay():
    seq = [
        {"move": "R"}, {"move": "U"},
        {"delay": 500},
        {"move": "F"}, {"move": "D"},
        {"delay": 50},
        {"move": "L"},
    ]
    r = requests.post(f"{BASE}/api/moves", json=seq, headers={"Content-Type": "application/json"})
    print(f"  inline delay test -> {r.text.strip()}")


print("魔方键盘控制")
print("  u/r/f/d/l/b = 顺时针   U/R/F/D/L/B = 逆时针")
print("  [ / ]        = 减/加延迟50ms  (当前={}ms)".format(DELAY))
print("  t            = 内联delay测试序列")
print("  q            = 退出")
print()

while True:
    ch = msvcrt.getch().decode('utf-8', errors='ignore')
    if ch == 'q':
        break
    elif ch == '[':
        set_delay(DELAY - 50)
    elif ch == ']':
        set_delay(DELAY + 50)
    elif ch == 't':
        test_inline_delay()
    elif ch in FACE_MAP:
        face = FACE_MAP[ch]
        dir = 1 if ch.isupper() else 0
        send_move(face, dir)
