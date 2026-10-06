"""统一协议抽象与事件类型。

参考 DCTimer-BLE `SmartCubeProtocol`：每个品牌实现一个协议类，负责连接后的
订阅、解密、解析与 ACK；解析结果统一为下面的事件对象，交由上层转发。

转动统一内部编码对齐 DCTimer：move = face * 3 + pow，face 为 URFDLB (0-5)，
pow 为 0=CW / 1=180? / 2=CCW。转发到 HTTP 时映射为 {face, dir}：
    dir=0 → CW，dir=1 → CCW（pow==2 即 CCW）。
"""
from __future__ import annotations

from dataclasses import dataclass

FACE_NAMES = ["U", "R", "F", "D", "L", "B"]


@dataclass
class MoveEvent:
    """一次转动。face 0-5 = URFDLB；dir 0=CW，1=CCW。"""

    face: int
    dir: int
    elapsed_ms: int = 0

    @property
    def notation(self) -> str:
        return "U R F D L B".split()[self.face] + ("" if self.dir == 0 else "'")


@dataclass
class StateEvent:
    """一次完整状态（54 字符 kociemba 面片串，URFDLB 顺序）。"""

    facelets: str


@dataclass
class GyroEvent:
    """归一化四元数 (x, y, z, w)。"""

    x: float
    y: float
    z: float
    w: float


@dataclass
class BatteryEvent:
    percent: int


class SmartCubeProtocol:
    """实体魔方协议接口。

    生命周期：
        start(conn, service, device_name, device_mac) 建立订阅并发起初始化请求
        on_notification(uuid, data)                    处理通知/指示
        on_disconnected()                              连接断开清理
    """

    #: 品牌标识（"gan" / "moyu32" / "qiyi"），用于日志。
    brand: str = "unknown"
    #: 目标服务 UUID（连接后据此复核选定协议）。
    service_uuid: str = ""

    def __init__(self, sink):
        # sink: 事件回调，形如 sink(MoveEvent | StateEvent | GyroEvent | BatteryEvent)
        self._sink = sink
        self._notifications_ready = False

    # --- 子类需实现 ---
    def start(self, conn, service, device_name: str, device_mac: str) -> bool:
        """连接后初始化。conn 为 ble_client 提供的写/订阅句柄，service 为已定位服务。"""
        raise NotImplementedError

    def on_notification(self, uuid: str, data: bytes) -> None:
        """收到 notify/indicate 数据。"""
        raise NotImplementedError

    def on_characteristic_write(self, uuid: str) -> None:
        """写完成回调（用于请求队列推进）。默认无操作。"""
        return None

    def on_mtu_changed(self, mtu: int) -> None:
        """MTU 协商完成。默认立即视为通知就绪。"""
        self.on_notifications_ready()

    def clear(self) -> None:
        """断开时清理状态。"""
        self._notifications_ready = False

    # --- 供子类调用 ---
    def on_notifications_ready(self) -> None:
        """订阅完成后由子类/连接层调用，触发初始化请求。子类覆写后需置位。"""
        self._notifications_ready = True

    @property
    def notifications_ready(self) -> bool:
        return self._notifications_ready

    # --- 事件发射 ---
    def emit(self, event) -> None:
        if self._sink is not None:
            self._sink(event)

    def emit_move(self, face: int, dir_: int, elapsed_ms: int = 0) -> None:
        self.emit(MoveEvent(face, dir_, elapsed_ms))
