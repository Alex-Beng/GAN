"""奇艺 QY-QYSC 智能魔方协议解析。

移植自 DCTimer-BLE `QiyiCubeProtocol`：
    服务 0000fff0-... 特征 0000fff6-...
    AES-128-ECB 固定密钥 + CRC16-Modbus
    帧头 0xfe, opcode 0x02=hello 0x03=状态变化 0x04=同步确认
    陀螺仪帧 0xcc 0x10
"""
from __future__ import annotations

import asyncio
import logging

from .crypto import QiyiCipher, crc16_modbus
from .protocol import SmartCubeProtocol

logger = logging.getLogger(__name__)

QIYI_CUBE_UUID = "0000fff6-0000-1000-8000-00805f9b34fb"
QIYI_WRITE_UUID = "0000fff5-0000-1000-8000-00805f9b34fb"

MOVE_AXIS_MAP = [4, 1, 3, 0, 2, 5]
DEVICE_TIME_SCALE = 1.6
MAX_MOVE_DELTA_MS = 0xFFFF

GYRO_HEADER = 0xCC
GYRO_LENGTH = 0x10
GYRO_CRC_LENGTH = 14
GYRO_SCALE = 1000.0

QIYI_HISTORY_SLOT_COUNT = 11
QIYI_HISTORY_SLOT_START = 36
QIYI_HISTORY_SLOT_SIZE = 5


def _fallback_mac(device_name: str) -> str | None:
    if not device_name:
        return None
    n = device_name.strip().upper()
    if not (n.startswith("QY-QYSC-") or n.startswith("XMD-TORNADOV4-I-")):
        return None
    suffix = n[-4:]
    if len(suffix) != 4 or any(c not in "0123456789ABCDEF" for c in suffix):
        return None
    return f"CC:A3:00:00:{suffix[:2]}:{suffix[2:]}"


def _read_u32(data: bytes, offset: int) -> int:
    return (
        ((data[offset] & 0xFF) << 24)
        | ((data[offset + 1] & 0xFF) << 16)
        | ((data[offset + 2] & 0xFF) << 8)
        | (data[offset + 3] & 0xFF)
    )


def _convert_move(raw_move: int) -> int:
    """奇艺 raw move (1-12) → DCTimer 编码 face*3+pow；非法返回 -1。"""
    if raw_move <= 0:
        return -1
    axis_index = (raw_move - 1) >> 1
    if axis_index < 0 or axis_index >= len(MOVE_AXIS_MAP):
        return -1
    axis = MOVE_AXIS_MAP[axis_index]
    power = 0 if (raw_move & 1) == 0 else 2
    return axis * 3 + power


class QiyiProtocol(SmartCubeProtocol):
    brand = "qiyi"
    service_uuid = "0000fff0-0000-1000-8000-00805f9b34fb"

    def __init__(self, sink):
        super().__init__(sink)
        self._conn = None
        self._cipher = QiyiCipher()
        self._device_name = ""
        self._device_mac = ""
        self._fallback_mac: str | None = None
        self._active_mac = ""
        self._hello_received = False
        self._fallback_sent = False
        self._request_queue: list[bytes] = []
        self._write_pending = False
        self._mtu_ready = False
        self._last_timestamp = -1

    def start(self, conn, service, device_name: str, device_mac: str) -> bool:
        self._conn = conn
        self._device_name = device_name or ""
        self._device_mac = device_mac
        self._active_mac = device_mac
        self._fallback_mac = _fallback_mac(self._device_name)
        logger.info("QiYi init (mac=%s, fallback=%s)", device_mac, self._fallback_mac)
        asyncio.ensure_future(self._subscribe())
        return True

    async def _subscribe(self) -> None:
        try:
            await self._conn.subscribe(QIYI_CUBE_UUID)
            self._mtu_ready = True
            self.on_notifications_ready()
        except Exception as e:
            logger.error("QiYi 订阅失败: %s", e)

    def on_notifications_ready(self) -> None:
        super().on_notifications_ready()
        if not self._active_mac:
            return
        self._enqueue_hello(self._active_mac, "初始化")
        if self._fallback_mac and self._fallback_mac.lower() != self._active_mac.lower():
            asyncio.ensure_future(self._schedule_fallback_hello())

    async def _schedule_fallback_hello(self) -> None:
        await asyncio.sleep(1.5)
        if self._hello_received or self._fallback_sent or not self._fallback_mac:
            return
        if self._fallback_mac.lower() == self._active_mac.lower():
            return
        self._fallback_sent = True
        self._active_mac = self._fallback_mac
        logger.warning("QiYi 使用备用 MAC: %s", self._fallback_mac)
        self._enqueue_hello(self._active_mac, "备用 MAC")

    def _enqueue_hello(self, mac: str, reason: str) -> None:
        logger.info("QiYi hello (%s) mac=%s", reason, mac)
        self._enqueue_message(self._build_hello_content(mac), priority=True)

    # --- 帧构造 ---
    def _build_hello_content(self, mac: str) -> bytes:
        mac_bytes = bytes(int(x, 16) for x in mac.split(":"))
        header = [0x00, 0x6B, 0x01, 0x00, 0x00, 0x22, 0x06, 0x00, 0x02, 0x08, 0x00]
        content = bytearray(header)
        for i in range(6):
            content.append(mac_bytes[5 - i])
        return bytes(content)

    def _enqueue_message(self, content: bytes, priority: bool = False) -> None:
        msg = self._build_message(content)
        if priority:
            self._request_queue.insert(0, msg)
        else:
            self._request_queue.append(msg)
        asyncio.ensure_future(self._send_next())

    def _build_message(self, content: bytes) -> bytes:
        msg_length = 4 + len(content)
        padded = ((msg_length + 15) // 16) * 16
        msg = bytearray(padded)
        msg[0] = 0xFE
        msg[1] = msg_length
        msg[2:2 + len(content)] = content
        crc = crc16_modbus(msg, 2 + len(content))
        msg[2 + len(content)] = crc & 0xFF
        msg[3 + len(content)] = (crc >> 8) & 0xFF
        return bytes(msg)

    async def _send_next(self) -> None:
        if not self._mtu_ready or self._write_pending or not self._request_queue or self._conn is None:
            return
        req = self._request_queue.pop(0)
        try:
            encoded = self._cipher.encrypt(req)
            self._write_pending = True
            await self._conn.write(QIYI_WRITE_UUID, encoded, response=False)
        except Exception as e:
            logger.error("QiYi 写入失败: %s", e)
        finally:
            self._write_pending = False
            if self._request_queue:
                asyncio.ensure_future(self._send_next())

    # --- 通知处理 ---
    def on_notification(self, uuid: str, data: bytes) -> None:
        if uuid.lower() != QIYI_CUBE_UUID.lower():
            return
        if not data or len(data) % 16 != 0:
            return
        try:
            decoded = self._cipher.decrypt(data)
            if len(decoded) >= 2 and decoded[0] == GYRO_HEADER and decoded[1] == GYRO_LENGTH:
                self._handle_gyro(decoded)
                return
            msg_length = decoded[1]
            if msg_length < 3 or msg_length > len(decoded):
                return
            msg = bytes(decoded[:msg_length])
            if crc16_modbus(msg, len(msg)) != 0:
                return
            if msg[0] != 0xFE:
                return
            opcode = msg[2]
            timestamp = _read_u32(msg, 3)
            if opcode == 0x02:
                self._handle_hello(msg, timestamp)
            elif opcode == 0x03:
                self._handle_state_change(msg, timestamp)
            elif opcode == 0x04:
                self._handle_sync(msg, timestamp)
        except Exception as e:
            logger.error("QiYi 解析失败: %s", e)

    def _handle_hello(self, msg: bytes, timestamp: int) -> None:
        self._hello_received = True
        self._send_ack(msg)
        if len(msg) < 36:
            return
        facelet = _parse_facelet(msg, 7)
        self.emit(_state_event(facelet))
        self._last_timestamp = timestamp
        self.emit(_battery_event(msg[35]))

    def _handle_state_change(self, msg: bytes, timestamp: int) -> None:
        self._hello_received = True
        self._send_ack(msg)
        if len(msg) < 36:
            return
        prev = self._last_timestamp
        for raw, ts in self._collect_moves(msg, prev, timestamp):
            mv = _convert_move(raw)
            if mv < 0:
                continue
            face = mv // 3
            pow_ = mv % 3
            self.emit_move(face, 0 if pow_ == 0 else 1, self._calc_delta(ts))
        self._last_timestamp = max(self._last_timestamp, timestamp)
        self.emit(_battery_event(msg[35]))

    def _handle_sync(self, msg: bytes, timestamp: int) -> None:
        if len(msg) < 36:
            return
        facelet = _parse_facelet(msg, 7)
        self.emit(_state_event(facelet))
        self._last_timestamp = timestamp
        self.emit(_battery_event(msg[35]))

    def _send_ack(self, msg: bytes) -> None:
        if len(msg) >= 7:
            self._enqueue_message(bytes(msg[2:7]), priority=False)

    @staticmethod
    def _collect_moves(msg: bytes, last_ts: int, frame_ts: int) -> list[tuple[int, int]]:
        if len(msg) < 35:
            return []
        candidates = [(msg[34], _read_u32(msg, 3))]
        for i in range(QIYI_HISTORY_SLOT_COUNT):
            offset = QIYI_HISTORY_SLOT_START + QIYI_HISTORY_SLOT_SIZE * i
            if offset + QIYI_HISTORY_SLOT_SIZE > len(msg):
                break
            if all(msg[offset + j] == 0xFF for j in range(QIYI_HISTORY_SLOT_SIZE)):
                continue
            candidates.append((msg[offset + 4], _read_u32(msg, offset)))
        candidates.sort(key=lambda c: c[1])
        result = []
        for raw, ts in candidates:
            if ts <= last_ts or ts > frame_ts:
                continue
            if _convert_move(raw) < 0:
                continue
            if (raw, ts) not in result:
                result.append((raw, ts))
        return result

    def _calc_delta(self, ts: int) -> int:
        if self._last_timestamp < 0:
            self._last_timestamp = ts
            return 0
        raw = ts - self._last_timestamp
        self._last_timestamp = ts
        if raw <= 0:
            return 0
        delta = round(raw / DEVICE_TIME_SCALE)
        return min(max(delta, 0), MAX_MOVE_DELTA_MS)

    def _handle_gyro(self, msg: bytes) -> None:
        if len(msg) < GYRO_LENGTH:
            return
        expected = crc16_modbus(msg, GYRO_CRC_LENGTH)
        actual = (msg[14] & 0xFF) | ((msg[15] & 0xFF) << 8)
        if expected != actual:
            return

        def i16(off):
            v = (msg[off] << 8) | msg[off + 1]
            return v - 0x10000 if v >= 0x8000 else v

        x = i16(6) / GYRO_SCALE
        y = i16(8) / GYRO_SCALE
        z = i16(10) / GYRO_SCALE
        w = i16(12) / GYRO_SCALE
        norm = (x * x + y * y + z * z + w * w) ** 0.5
        if norm < 1e-6:
            return
        self.emit(_gyro_event(x / norm, y / norm, z / norm, w / norm))


def _parse_facelet(msg: bytes, offset: int) -> str:
    state = []
    for i in range(54):
        value = (msg[offset + (i >> 1)] >> ((i & 1) << 2)) & 0x0F
        state.append("LRDUFB"[value])
    return "".join(state)


def _state_event(facelet: str):
    from .protocol import StateEvent
    return StateEvent(facelet)


def _battery_event(percent: int):
    from .protocol import BatteryEvent
    return BatteryEvent(percent)


def _gyro_event(x: float, y: float, z: float, w: float):
    from .protocol import GyroEvent
    return GyroEvent(x, y, z, w)
