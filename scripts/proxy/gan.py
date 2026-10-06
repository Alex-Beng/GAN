"""GAN 智能魔方协议解析（V2 / V3 / V4）。

移植自 DCTimer-BLE `GanCubeProtocol`：
    V2: 服务 6e400001-...  通知 mode 1=陀螺仪 / 2=转动 / 4=面片 / 9=电量
    V3: 服务 8653000a-...  magic 0x55, mode 1=转动 / 2=面片 / 6=history / 16=电量
    V4: 服务 00000010-...  mode 0xEC=陀螺仪 / 0x01=转动 / 0xED=面片 / 0xD1=history / 0xEF=电量

统一转动编码 face*3+pow（pow 0=CW / 2=CCW），emit 时映射为 {face, dir}。
"""
from __future__ import annotations

import asyncio
import logging

from .crypto import GanCipher
from .protocol import SmartCubeProtocol

logger = logging.getLogger(__name__)

# V2 请求
V2_READ_UUID = "28be4cb6-cd67-11e9-a32f-2a2ae2dbcce4"
V2_WRITE_UUID = "28be4a4a-cd67-11e9-a32f-2a2ae2dbcce4"
# V3
V3_READ_UUID = "8653000b-43e6-47b7-9cb0-5fc21d4ae340"
V3_WRITE_UUID = "8653000c-43e6-47b7-9cb0-5fc21d4ae340"
# V4
V4_READ_UUID = "0000fff6-0000-1000-8000-00805f9b34fb"
V4_WRITE_UUID = "0000fff5-0000-1000-8000-00805f9b34fb"

AXIS_MASKS = [2, 32, 8, 1, 16, 4]
MAX_MOVE_DELTA_MS = 0xFFFF

_CHR_BY_VARIANT = {
    "v2": (V2_READ_UUID, V2_WRITE_UUID),
    "v3": (V3_READ_UUID, V3_WRITE_UUID),
    "v4": (V4_READ_UUID, V4_WRITE_UUID),
}


def _bitstring(data: bytes) -> str:
    return "".join(f"{b:08b}" for b in data)


def _bits(bits: str, start: int, length: int) -> int:
    return int(bits[start:start + length], 2)


class GanProtocol(SmartCubeProtocol):
    brand = "gan"

    def __init__(self, sink, variant: str):
        super().__init__(sink)
        self.variant = variant
        self.read_uuid, self.write_uuid = _CHR_BY_VARIANT[variant]
        self._conn = None
        self._cipher: GanCipher | None = None
        self._request_queue: list[bytes] = []
        self._write_pending = False
        self._prev_move_cnt = -1
        self._last_device_ts = -1

    def start(self, conn, service, device_name: str, device_mac: str) -> bool:
        self._conn = conn
        ai_cube = self.variant == "v2" and device_name.upper().startswith("AICUBE")
        self._cipher = GanCipher(device_mac, ai_cube=ai_cube)
        logger.info("GAN %s cipher init (mac=%s, aicube=%s)", self.variant, device_mac, ai_cube)

        asyncio.ensure_future(self._subscribe())
        return True

    async def _subscribe(self) -> None:
        try:
            await self._conn.subscribe(self.read_uuid)
            self.on_notifications_ready()
        except Exception as e:
            logger.error("GAN 订阅失败: %s", e)

    def on_notifications_ready(self) -> None:
        super().on_notifications_ready()
        self._enqueue_initial()

    def _enqueue_initial(self) -> None:
        if self.variant == "v2":
            self._enqueue(self._v2_req(4))
            self._enqueue(self._v2_req(5))
            self._enqueue(self._v2_req(9))
        elif self.variant == "v3":
            self._enqueue(self._v3_req(1))
            self._enqueue(self._v3_req(4))
            self._enqueue(self._v3_req(7))
        else:  # v4
            self._enqueue(self._v4_req(0xDD, 0x04, ed=0xED))  # facelets
            self._enqueue(self._v4_req(0xDF, 0x03))           # hardware
            self._enqueue(self._v4_req(0xDD, 0x04, ed=0xEF))  # battery

    # --- 请求构造 ---
    @staticmethod
    def _v2_req(opcode: int) -> bytes:
        b = bytearray(20)
        b[0] = opcode
        return bytes(b)

    @staticmethod
    def _v3_req(opcode: int) -> bytes:
        b = bytearray(16)
        b[0] = 0x68
        b[1] = opcode
        return bytes(b)

    @staticmethod
    def _v4_req(b0: int, b1: int, ed: int = 0) -> bytes:
        b = bytearray(20)
        b[0] = b0
        b[1] = b1
        if ed:
            b[3] = ed
        return bytes(b)

    def _enqueue(self, req: bytes) -> None:
        self._request_queue.append(req)
        asyncio.ensure_future(self._send_next())

    async def _send_next(self) -> None:
        if self._write_pending or not self._request_queue or self._conn is None:
            return
        req = self._request_queue.pop(0)
        try:
            encoded = self._cipher.encode(req)
            self._write_pending = True
            await self._conn.write(self.write_uuid, encoded, response=False)
        except Exception as e:
            logger.error("GAN 写入失败: %s", e)
        finally:
            self._write_pending = False
            if self._request_queue:
                asyncio.ensure_future(self._send_next())

    # --- 通知处理 ---
    def on_notification(self, uuid: str, data: bytes) -> None:
        if uuid.lower() != self.read_uuid.lower():
            return
        try:
            plain = self._cipher.decode(data)
            bits = _bitstring(plain)
            if self.variant == "v2":
                self._parse_v2(bits)
            elif self.variant == "v3":
                self._parse_v3(bits)
            else:
                self._parse_v4(bits)
        except Exception as e:
            logger.error("GAN 解析失败: %s", e)

    def clear(self) -> None:
        super().clear()
        self._request_queue.clear()

    # ---------- V2 ----------
    def _parse_v2(self, bits: str) -> None:
        mode = _bits(bits, 0, 4)
        if mode == 1:
            self._emit_gyro(bits, 4)
        elif mode == 2:
            self._v2_move(bits)
        elif mode == 4:
            self._v2_facelets(bits)
        elif mode == 9:
            self.emit(_battery_event(_bits(bits, 8, 8)))

    def _v2_facelets(self, bits: str) -> None:
        if self._prev_move_cnt != -1:
            return
        move_cnt = _bits(bits, 4, 8)
        facelet = self._parse_facelet(bits, 12, 33, 47, 91)
        if facelet:
            self.emit(_state_event(facelet))
            self._prev_move_cnt = move_cnt

    def _v2_move(self, bits: str) -> None:
        move_cnt = _bits(bits, 4, 8)
        if move_cnt == self._prev_move_cnt or self._prev_move_cnt == -1:
            return
        moves = []
        times = []
        for i in range(7):
            raw = _bits(bits, 12 + i * 5, 5)
            if raw >= 12:
                return
            moves.append(_convert_quarter(raw >> 1, raw & 1))
            times.append(_bits(bits, 47 + i * 16, 16))
        diff = (move_cnt - self._prev_move_cnt) & 0xFF
        self._prev_move_cnt = move_cnt
        for i in range(min(diff, len(moves)) - 1, -1, -1):
            self._emit_encoded(moves[i], times[i])

    # ---------- V3 ----------
    def _parse_v3(self, bits: str) -> None:
        magic = _bits(bits, 0, 8)
        mode = _bits(bits, 8, 8)
        length = _bits(bits, 16, 8)
        if magic != 0x55 or length <= 0:
            return
        if mode == 1:
            self._v3_move(bits)
        elif mode == 2:
            self._v3_facelets(bits)
        elif mode == 16:
            self.emit(_battery_event(_bits(bits, 24, 8)))

    def _v3_move(self, bits: str) -> None:
        move_cnt = ((_bits(bits, 64, 8) << 8) | _bits(bits, 56, 8)) & 0xFF
        if move_cnt == self._prev_move_cnt or self._prev_move_cnt == -1:
            return
        ts = _bits(bits, 48, 8)
        ts = (ts << 8) | _bits(bits, 40, 8)
        ts = (ts << 8) | _bits(bits, 32, 8)
        ts = (ts << 8) | _bits(bits, 24, 8)
        pow_ = _bits(bits, 72, 2)
        axis = _decode_axis(_bits(bits, 74, 6))
        if axis < 0:
            return
        delta = self._compute_delta(ts)
        self._prev_move_cnt = move_cnt
        self._emit_encoded(_convert_quarter(axis, pow_), delta)

    def _v3_facelets(self, bits: str) -> None:
        move_cnt = ((_bits(bits, 32, 8) << 8) | _bits(bits, 24, 8)) & 0xFF
        if self._prev_move_cnt != -1:
            return
        facelet = self._parse_facelet(bits, 40, 61, 77, 121)
        if facelet:
            self.emit(_state_event(facelet))
            self._prev_move_cnt = move_cnt
            self._last_device_ts = -1

    # ---------- V4 ----------
    def _parse_v4(self, bits: str) -> None:
        mode = _bits(bits, 0, 8)
        length = _bits(bits, 8, 8)
        if mode == 0xEC:
            self._emit_gyro(bits, 16)
        elif mode == 0x01:
            self._v4_move(bits)
        elif mode == 0xED:
            self._v4_facelets(bits)
        elif mode == 0xEF:
            self.emit(_battery_event(_bits(bits, 8 + length * 8, 8)))

    def _v4_move(self, bits: str) -> None:
        offset = 0
        while offset + 72 <= len(bits):
            if _bits(bits, offset, 8) != 0x01:
                break
            move_cnt = ((_bits(bits, offset + 56, 8) << 8) | _bits(bits, offset + 48, 8)) & 0xFF
            ts = _bits(bits, offset + 40, 8)
            ts = (ts << 8) | _bits(bits, offset + 32, 8)
            ts = (ts << 8) | _bits(bits, offset + 24, 8)
            ts = (ts << 8) | _bits(bits, offset + 16, 8)
            pow_ = _bits(bits, offset + 64, 2)
            axis = _decode_axis(_bits(bits, offset + 66, 6))
            if axis < 0:
                break
            if self._prev_move_cnt == -1:
                self._prev_move_cnt = (move_cnt - 1) & 0xFF
            if move_cnt != self._prev_move_cnt:
                delta = self._compute_delta(ts)
                self._prev_move_cnt = move_cnt
                self._emit_encoded(_convert_quarter(axis, pow_), delta)
            offset += 72

    def _v4_facelets(self, bits: str) -> None:
        facelet = self._parse_facelet(bits, 32, 53, 69, 113)
        if facelet:
            self.emit(_state_event(facelet))
            self._last_device_ts = -1

    # ---------- 公共工具 ----------
    def _parse_facelet(self, bits: str, cp0: int, co0: int, ep0: int, eo0: int) -> str | None:
        cp = [0] * 8
        co = [0] * 8
        ep = [0] * 12
        eo = [0] * 12
        cchk = 0xF00
        for i in range(7):
            perm = _bits(bits, cp0 + i * 3, 3)
            ori = _bits(bits, co0 + i * 2, 2)
            cchk -= ori << 3
            cchk ^= perm
            cp[i] = perm
            co[i] = ori
        cp[7] = cchk & 0x7
        co[7] = ((cchk & 0xFF8) % 24) >> 3
        echk = 0
        for i in range(11):
            perm = _bits(bits, ep0 + i * 4, 4)
            ori = _bits(bits, eo0 + i, 1)
            echk ^= (perm << 1) | ori
            ep[i] = perm
            eo[i] = ori
        ep[11] = echk >> 1
        eo[11] = echk & 1
        return cubie_to_facelet(cp, co, ep, eo)

    def _compute_delta(self, device_ts: int) -> int:
        if self._last_device_ts < 0:
            self._last_device_ts = device_ts
            return 0
        delta = device_ts - self._last_device_ts
        self._last_device_ts = device_ts
        if delta < 0:
            return 0
        return min(delta, MAX_MOVE_DELTA_MS)

    def _emit_encoded(self, move: int, elapsed_ms: int) -> None:
        """move = face*3+pow → MoveEvent(face, dir)。"""
        face = move // 3
        pow_ = move % 3
        self.emit_move(face, 0 if pow_ == 0 else 1, elapsed_ms)

    def _emit_gyro(self, bits: str, start: int) -> None:
        try:
            w = _decode_gyro(_bits(bits, start, 16))
            x = _decode_gyro(_bits(bits, start + 16, 16))
            y = _decode_gyro(_bits(bits, start + 32, 16))
            z = _decode_gyro(_bits(bits, start + 48, 16))
            norm = (x * x + y * y + z * z + w * w) ** 0.5
            if norm < 1e-6:
                return
            self.emit(_gyro_event(x / norm, z / norm, -y / norm, w / norm))
        except Exception:
            pass


def _decode_axis(mask: int) -> int:
    for i, m in enumerate(AXIS_MASKS):
        if m == mask:
            return i
    return -1


def _convert_quarter(axis: int, pow_: int) -> int:
    return axis * 3 + (0 if pow_ == 0 else 2)


def _decode_gyro(raw: int) -> float:
    sign = 1.0 if (raw & 0x8000) == 0 else -1.0
    return sign * (raw & 0x7FFF) / 0x7FFF


# 事件构造（延迟导入避免循环）
def _state_event(facelet: str):
    from .protocol import StateEvent
    return StateEvent(facelet)


def _battery_event(percent: int):
    from .protocol import BatteryEvent
    return BatteryEvent(percent)


def _gyro_event(x: float, y: float, z: float, w: float):
    from .protocol import GyroEvent
    return GyroEvent(x, y, z, w)


# Kociemba 面片还原（移植 gan-cube-emu protocol.rs / MOYU ble_cube.py）
_CORNER_FACELET = [
    [8, 9, 20], [6, 18, 38], [0, 36, 47], [2, 45, 11],
    [29, 26, 15], [27, 44, 24], [33, 53, 42], [35, 17, 51],
]
_CORNER_COLOR = [
    [0, 1, 2], [0, 2, 4], [0, 4, 5], [0, 5, 1],
    [3, 2, 1], [3, 4, 2], [3, 5, 4], [3, 1, 5],
]
_EDGE_FACELET = [
    [5, 10], [7, 19], [3, 37], [1, 46], [32, 16], [28, 25],
    [30, 43], [34, 52], [23, 12], [21, 41], [50, 39], [48, 14],
]
_EDGE_COLOR = [
    [0, 1], [0, 2], [0, 4], [0, 5], [3, 1], [3, 2],
    [3, 4], [3, 5], [2, 1], [2, 4], [5, 4], [5, 1],
]
_CENTER_FACELET = [4, 13, 22, 31, 40, 49]
_FACELET_COLORS = "URFDLB"


def cubie_to_facelet(cp, co, ep, eo) -> str | None:
    """角/棱位置与方向 → 54 字符 kociemba 面片串；非法状态返回 None。"""
    if sorted(cp) != list(range(8)) or sorted(ep) != list(range(12)):
        return None
    if sum(co) % 3 != 0 or sum(eo) % 2 != 0:
        return None
    f = ["U"] * 54
    for i in range(6):
        f[_CENTER_FACELET[i]] = _FACELET_COLORS[i]
    for i in range(8):
        j = cp[i]
        ori = co[i] % 3
        for k in range(3):
            f[_CORNER_FACELET[i][(k + ori) % 3]] = _FACELET_COLORS[_CORNER_COLOR[j][k]]
    for i in range(12):
        j = ep[i]
        ori = eo[i] % 2
        for k in range(2):
            f[_EDGE_FACELET[i][(k + ori) % 2]] = _FACELET_COLORS[_EDGE_COLOR[j][k]]
    return "".join(f)
