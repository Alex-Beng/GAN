"""BLE 连接层：连接、GATT 发现、按服务 UUID 复核协议、订阅、分发。

参考 DCTimer-BLE `BluetoothTools`：连接后 `onServicesDiscovered` 里用 GATT
服务 UUID 复核品牌/协议（比扫描时只看名字可靠，能区分 GAN V2/V3/V4），再创建
对应协议类并启动（订阅 + 初始化请求）。
"""
from __future__ import annotations

import asyncio
import logging

from bleak import BleakClient

from . import scanner
from .protocol import SmartCubeProtocol

logger = logging.getLogger(__name__)

# 服务 UUID → (品牌, 具体协议键)
SERVICE_TO_PROTOCOL = {
    scanner.GAN_V2_SERVICE.lower(): ("gan", "v2"),
    scanner.GAN_V3_SERVICE.lower(): ("gan", "v3"),
    scanner.GAN_V4_SERVICE.lower(): ("gan", "v4"),
    scanner.MOYU32_SERVICE.lower(): ("moyu32", ""),
    scanner.QIYI_SERVICE.lower(): ("qiyi", ""),
}

# 特征 UUID → (品牌, 具体协议键)（服务 UUID 未命中时兜底）
CHR_TO_PROTOCOL = {
    scanner.GAN_V2_CHR.lower(): ("gan", "v2"),
    scanner.GAN_V3_CHR.lower(): ("gan", "v3"),
    scanner.GAN_V4_CHR.lower(): ("gan", "v4"),
    scanner.MOYU32_CHR.lower(): ("moyu32", ""),
    scanner.QIYI_CHR.lower(): ("qiyi", ""),
}

CCCD_UUID = "00002902-0000-1000-8000-00805f9b34fb"


def _uuid_str(uuid) -> str:
    return str(uuid).lower()


def resolve_protocol(services) -> tuple[str, str] | None:
    """按 GATT 服务/特征 UUID 复核具体协议，返回 (brand, variant)。"""
    svc_uuids = set()
    chr_uuids = set()
    for svc in services:
        svc_uuids.add(_uuid_str(svc.uuid))
        for ch in svc.characteristics:
            chr_uuids.add(_uuid_str(ch.uuid))

    for uuid, proto in SERVICE_TO_PROTOCOL.items():
        if uuid in svc_uuids:
            return proto
    for uuid, proto in CHR_TO_PROTOCOL.items():
        if uuid in chr_uuids:
            return proto
    return None


def find_service(services, target_uuid: str):
    target = target_uuid.lower()
    for svc in services:
        if _uuid_str(svc.uuid) == target:
            return svc
    return None


class CubeConnection:
    """封装单个实体魔方的 bleak 连接与协议分发。"""

    def __init__(self, device: scanner.CubeDevice, sink, source: str = "auto"):
        self.device = device
        self._sink = sink
        self._source = source
        self._client: BleakClient | None = None
        self._proto: SmartCubeProtocol | None = None
        self._mac = device.address  # 协议地址（部分品牌用广播 MAC）
        self._loop = asyncio.get_event_loop()
        self.connected = False

    @property
    def proto(self) -> SmartCubeProtocol | None:
        return self._proto

    async def connect(self) -> bool:
        self._client = BleakClient(self.device.address, timeout=15.0)
        await self._client.connect()
        if not self._client.is_connected:
            logger.error("连接失败: %s", self.device.address)
            return False
        self.connected = True
        logger.info("已连接: %s (%s)", self.device.name, self.device.address)

        services = self._client.services
        resolved = resolve_protocol(services)
        if resolved is None:
            # 按名字降级
            brand = self.device.brand
            if brand == "unknown":
                logger.error("无法识别协议: %s", self.device.name)
                await self.disconnect()
                return False
            resolved = (brand, "")
            logger.warning("服务 UUID 未命中，按名字降级为 %s", brand)

        brand, variant = resolved
        logger.info("协议复核: brand=%s variant=%s", brand, variant)

        self._proto = self._create_protocol(brand, variant)
        if self._proto is None:
            logger.error("不支持的协议: %s/%s", brand, variant)
            await self.disconnect()
            return False

        service = self._find_target_service(brand, variant, services)
        if service is None:
            logger.error("未找到目标服务 (%s/%s)", brand, variant)
            await self.disconnect()
            return False

        ok = self._proto.start(self, service, self.device.name, self._mac)
        if not ok:
            logger.error("协议初始化失败")
            await self.disconnect()
            return False
        return True

    def _create_protocol(self, brand: str, variant: str) -> SmartCubeProtocol | None:
        if brand == "gan":
            from .gan import GanProtocol
            return GanProtocol(self._sink, variant)
        if brand == "moyu32":
            from .moyu32 import Moyu32Protocol
            return Moyu32Protocol(self._sink)
        if brand == "qiyi":
            from .qiyi import QiyiProtocol
            return QiyiProtocol(self._sink)
        return None

    def _find_target_service(self, brand: str, variant: str, services):
        if brand == "gan":
            mapping = {"v2": scanner.GAN_V2_SERVICE, "v3": scanner.GAN_V3_SERVICE, "v4": scanner.GAN_V4_SERVICE}
            return find_service(services, mapping[variant])
        if brand == "moyu32":
            return find_service(services, scanner.MOYU32_SERVICE)
        if brand == "qiyi":
            return find_service(services, scanner.QIYI_SERVICE)
        return None

    # --- 供协议类使用的连接句柄 ---
    async def subscribe(self, uuid: str, indicate: bool = False) -> None:
        def _cb(_sender, data: bytearray):
            if self._proto is not None:
                self._proto.on_notification(uuid, bytes(data))

        await self._client.start_notify(uuid, _cb)

    async def write(self, uuid: str, data: bytes, response: bool = False) -> None:
        await self._client.write_gatt_char(uuid, data, response=response)

    async def disconnect(self) -> None:
        self.connected = False
        if self._proto is not None:
            self._proto.clear()
        if self._client is not None:
            try:
                await self._client.disconnect()
            except Exception as e:
                logger.debug("断开异常: %s", e)
        logger.info("已断开: %s", self.device.address)
