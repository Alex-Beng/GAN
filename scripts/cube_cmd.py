import requests
import sys
import readline

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://192.168.50.246"

FACE_MAP = {
    'u': 0, 'r': 1, 'f': 2, 'd': 3, 'l': 4, 'b': 5,
    'U': 0, 'R': 1, 'F': 2, 'D': 3, 'L': 4, 'B': 5,
}

def send_moves(moves):
    if not moves:
        return
    r = requests.post(f"{BASE}/api/moves", json=moves)
    print(f"-> {r.text}")

def parse_line(line):
    import re
    tokens = re.findall(r"[URFDLBurfdlb]['2]?", line.strip(), re.IGNORECASE)
    moves = []
    names = ['U','R','F','D','L','B']
    for t in tokens:
        base = t[0].upper()
        if base not in FACE_MAP:
            print(f"忽略无效: {t}")
            continue
        face = FACE_MAP[base]
        suffix = t[1:] if len(t) > 1 else ''
        if suffix == "'":
            moves.append({"face": face, "dir": 1})
            print(f"  {names[face]}'")
        elif suffix == "2":
            moves.append({"face": face, "dir": 0})
            moves.append({"face": face, "dir": 0})
            print(f"  {names[face]}2")
        elif suffix == '':
            moves.append({"face": face, "dir": 0})
            print(f"  {names[face]}")
        else:
            print(f"忽略无效: {t}")
    return moves

print("魔方键盘控制 (输入如: R U R' U' F2, q退出)")
while True:
    try:
        line = input("> ")
    except EOFError:
        break
    if line.strip().lower() == 'q':
        break
    moves = parse_line(line)
    send_moves(moves)
