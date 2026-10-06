"""魔域 WCU_MY3 智能魔方协议解析。

移植自 DCTimer-BLE `Moyu32CubeProtocol`：
    服务 0783b03e-7735-b5a0-1760-a305d2795cb0
    读   ...5cb1 (notify)  写 ...5cb2
    AES-CBC (MAC 派生, 与 GAN 同方案)
    opcode 161=信息 163=面片 164=电量 165=转动 171/172=陀螺仪
"""
from __future__ import annotations

import asyncio
import logging
import struct

from .crypto import Moyu32Cipher
from .protocol import SmartCubeProtocol

logger = logging.getLogger(__name__)

MOYU32_READ_UUID = "0783b03e-7735-b5a0-1760-a305d2795cb1"
MOYU32_WRITE_UUID = "0783b03e-7735-b5a0-1760-a305d2795cb2"

GYRO_SCALE = 1073741824.0
SOLVED_STATE = "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"


def _bitstring(data: bytes) -> str:
    return "".join(f"{b:08b}" for b in data)


def _bits(bits: str, start: int, length: int) -> int:
    return int(bits[start:start + length], 2)


def _fallback_mac(device_name: str) -> str | None:
    """WCU_MY32_XXXX → CF:30:16:00:XX:XX（DCTimer getFallbackMac）。"""
    if not device_name:
        return None
    n = device_name.strip().upper()
    if not n.startswith("WCU_MY32_"):
        return None
    suffix = n[-4:]
    if len(suffix) != 4 or any(c not in "0123456789ABCDEF" for c in suffix):
        return None
    return f"CF:30:16:00:{suffix[:2]}:{suffix[2:]}"


class Moyu32Protocol(SmartCubeProtocol):
    brand = "moyu32"
    service_uuid = "0783b03e-7735-b5a0-1760-a305d2795cb0"

    def __init__(self, sink):
        super().__init__(sink)
        self._conn = None
        self._cipher: Moyu32Cipher | None = None
        self._device_name = ""
        self._device_mac = ""
        self._fallback_tried = False
        self._request_queue: list[bytes] = []
        self._write_pending = False
        self._prev_move_cnt = -1

    def start(self, conn, service, device_name: str, device_mac: str) -> bool:
        self._conn = conn
        self._device_name = device_name or ""
        self._device_mac = device_mac
        self._cipher = Moyu32Cipher(device_mac)
        logger.info("MoYu32 cipher init (mac=%s)", device_mac)
        asyncio.ensure_future(self._subscribe())
        return True

    async def _subscribe(self) -> None:
        try:
            await self._conn.subscribe(MOYU32_READ_UUID)
            self.on_notifications_ready()
        except Exception as e:
            logger.error("MoYu32 订阅失败: %s", e)

    def on_notifications_ready(self) -> None:
        super().on_notifications_ready()
        for opcode in (161, 163, 164, 161, 163, 164):
            self._enqueue(self._simple_req(opcode))
        self._enqueue(self._gyro_enable_req())
        self._enqueue(self._simple_req(163))

    @staticmethod
    def _simple_req(opcode: int) -> bytes:
        b = bytearray(20)
        b[0] = opcode
        return bytes(b)

    @staticmethod
    def _gyro_enable_req() -> bytes:
        b = bytearray(20)
        b[0] = 172
        b[2] = 1
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
            await self._conn.write(MOYU32_WRITE_UUID, encoded, response=False)
        except Exception as e:
            logger.error("MoYu32 写入失败: %s", e)
        finally:
            self._write_pending = False
            if self._request_queue:
                asyncio.ensure_future(self._send_next())

    def on_notification(self, uuid: str, data: bytes) -> None:
        if uuid.lower() != MOYU32_READ_UUID.lower():
            return
        try:
            decoded = self._cipher.decode(data)
            if len(decoded) >= 17 and decoded[0] == 171:
                self._handle_gyro(decoded)
                return
            bits = _bitstring(decoded)
            msg_type = _bits(bits, 0, 8)
            if msg_type == 163:
                self._handle_state(bits)
            elif msg_type == 164:
                self.emit(_battery_event(_bits(bits, 8, 8)))
            elif msg_type == 165:
                self._handle_move(bits)
            elif msg_type == 161:
                logger.debug("MoYu32 设备信息")
            else:
                logger.debug("MoYu32 未知消息: %s", msg_type)
                if self._prev_move_cnt == -1:
                    self._try_fallback()
        except Exception as e:
            logger.error("MoYu32 解析失败: %s", e)

    def _handle_state(self, bits: str) -> None:
        if self._prev_move_cnt != -1:
            return
        move_cnt = _bits(bits, 152, 8)
        facelet = self._parse_facelet(bits[8:152])
        if facelet:
            self.emit(_state_event(facelet))
            self._prev_move_cnt = move_cnt

    def _handle_move(self, bits: str) -> None:
        move_cnt = _bits(bits, 88, 8)
        if self._prev_move_cnt == -1 or move_cnt == self._prev_move_cnt:
            return
        times = []
        raw_moves = []
        for i in range(5):
            times.append(_bits(bits, 8 + i * 16, 16))
            raw = _bits(bits, 96 + i * 5, 5)
            if raw >= 12:
                return
            raw_moves.append(raw)
        diff = (move_cnt - self._prev_move_cnt) & 0xFF
        self._prev_move_cnt = move_cnt
        for i in range(min(diff, len(raw_moves)) - 1, -1, -1):
            face, dir_ = _convert_move(raw_moves[i])
            self.emit_move(face, dir_, times[i])

    def _handle_gyro(self, decoded: bytes) -> None:
        if len(decoded) < 17:
            return
        w, x, y, z = struct.unpack_from("<iiii", decoded, 1)
        w, x, y, z = w / GYRO_SCALE, x / GYRO_SCALE, y / GYRO_SCALE, z / GYRO_SCALE
        length = (w * w + x * x + y * y + z * z) ** 0.5
        if length <= 0:
            return
        self.emit(_gyro_event(x / length, z / length, -y / length, w / length))

    def _try_fallback(self) -> None:
        if self._fallback_tried:
            return
        self._fallback_tried = True
        fb = _fallback_mac(self._device_name)
        if not fb or fb.lower() == self._device_mac.lower():
            return
        try:
            self._cipher = Moyu32Cipher(fb)
            self._device_mac = fb
            self._prev_move_cnt = -1
            self._request_queue.clear()
            self._enqueue(self._simple_req(163))
            self._enqueue(self._simple_req(164))
            logger.warning("MoYu32 使用备用 MAC: %s", fb)
        except Exception as e:
            logger.error("MoYu32 备用 MAC 失败: %s", e)

    def _parse_facelet(self, facelet_bits: str) -> str | None:
        try:
            state = []
            faces = [2, 5, 0, 3, 4, 1]
            for i in range(6):
                face = facelet_bits[faces[i] * 24: faces[i] * 24 + 24]
                for j in range(8):
                    state.append("FBUDLR"[_bits(face, j * 3, 3)])
                    if j == 3:
                        state.append("FBUDLR"[faces[i]])
            return "".join(state)
        except Exception:
            return None


def _convert_move(raw_move: int) -> tuple[int, int]:
    """"FBUDLR" 面序 + 奇偶 → (face URFDLB, dir 0=CW/1=CCW)。"""
    face = "FBUDLR"[raw_move >> 1]
    face_idx = "URFDLB".index(face)
    dir_ = 0 if (raw_move & 1) == 0 else 1
    return face_idx, dir_


def _state_event(facelet: str):
    from .protocol import StateEvent
    return StateEvent(facelet)


def _battery_event(percent: int):
    from .protocol import BatteryEvent
    return BatteryEvent(percent)


def _gyro_event(x: float, y: float, z: float, w: float):
    from .protocol import GyroEvent
    return GyroEvent(x, y, z, w)
