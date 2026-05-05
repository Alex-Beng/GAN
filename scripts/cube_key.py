import requests
import sys
import msvcrt

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://192.168.50.246"

FACE_MAP = {
    'u': 0, 'r': 1, 'f': 2, 'd': 3, 'l': 4, 'b': 5,
    'U': 0, 'R': 1, 'F': 2, 'D': 3, 'L': 4, 'B': 5,
}

NAMES = ['U','R','F','D','L','B']

def send_move(face, dir):
    r = requests.post(f"{BASE}/api/move", json={"face": face, "dir": dir})
    label = NAMES[face] + ("'" if dir else "")
    print(f"  {label} -> {r.text.strip()}")

print("魔方键盘控制 (小写=顺时针, 大写=逆时针, q退出)")
while True:
    ch = msvcrt.getch().decode('utf-8', errors='ignore')
    if ch == 'q':
        break
    if ch in FACE_MAP:
        face = FACE_MAP[ch]
        dir = 1 if ch.isupper() else 0
        send_move(face, dir)
