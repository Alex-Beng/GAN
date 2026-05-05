import requests
import sys
import json

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://192.168.50.246"

def post(path, data):
    r = requests.post(f"{BASE}{path}", json=data)
    print(f"POST {path} {data} -> {r.status_code}: {r.text}")

def test_move():
    for face, name in enumerate(["U","R","F","D","L","B"]):
        post("/api/move", {"face": face, "dir": 0})
        post("/api/move", {"face": face, "dir": 1})

def test_moves():
    moves = [{"face": 0, "dir": 0}, {"face": 1, "dir": 1}, {"face": 2, "dir": 0}]
    post("/api/moves", moves)

def test_wifi_ap():
    post("/api/wifi/mode", {"ap": 1})

def test_wifi_sta():
    ssid = input("STA SSID: ")
    pwd = input("STA Password: ")
    post("/api/wifi/mode", {"ap": 0, "ssid": ssid, "pass": pwd})

while True:
    print("\n1. 单步转动  2. 批量转动  3. 切AP  4. 切STA  q. 退出")
    c = input("> ").strip()
    if c == "1": test_move()
    elif c == "2": test_moves()
    elif c == "3": test_wifi_ap()
    elif c == "4": test_wifi_sta()
    elif c == "q": break
