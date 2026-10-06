"""BLE 扫描与品牌初筛（按设备名）。

参考 DCTimer-BLE `BluetoothTools.guessDeviceType`：扫描阶段只有设备名可用，
先按名字初筛品牌；连接后再用 GATT 服务 UUID 复核具体协议（见 ble_client）。
"""
from __future__ import annotations

import asyncio
from dataclasses import dataclass, field

from bleak import BleakScanner

# 服务 UUID（连接后复核用）
GAN_V2_SERVICE = "6e400001-b5a3-f393-e0a9-e50e24dc4179"
GAN_V3_SERVICE = "8653000a-43e6-47b7-9cb0-5fc21d4ae340"
GAN_V4_SERVICE = "00000010-0000-fff7-fff6-fff5fff4fff0"
MOYU32_SERVICE = "0783b03e-7735-b5a0-1760-a305d2795cb0"
QIYI_SERVICE = "0000fff0-0000-1000-8000-00805f9b34fb"

# 特征 UUID（服务 UUID 未命中时兜底反查）
GAN_V2_CHR = "28be4cb6-cd67-11e9-a32f-2a2ae2dbcce4"
GAN_V3_CHR = "8653000b-43e6-47b7-9cb0-5fc21d4ae340"
GAN_V4_CHR = "0000fff6-0000-1000-8000-00805f9b34fb"
MOYU32_CHR = "0783b03e-7735-b5a0-1760-a305d2795cb1"
QIYI_CHR = "0000fff6-0000-1000-8000-00805f9b34fb"


@dataclass
class CubeDevice:
    name: str
    address: str
    brand: str  # "gan" / "moyu32" / "qiyi" / "unknown"
    rssi: int | None = None
    manufacturer_data: dict = field(default_factory=dict)
    service_uuids: list = field(default_factory=list)


def guess_brand(name: str | None) -> str:
    """按设备名初筛品牌（DCTimer guessDeviceType 的魔方部分）。"""
    if not name:
        return "unknown"
    n = name.strip().upper()
    if n.startswith("WCU_MY3"):
        return "moyu32"
    if n.startswith("QY-QYSC") or n.startswith("XMD-TORNADOV4-I"):
        return "qiyi"
    if n.startswith("GAN") or n.startswith("MG") or n.startswith("AICUBE"):
        return "gan"
    return "unknown"


def _to_device(device, adv) -> CubeDevice:
    name = device.name or ""
    return CubeDevice(
        name=name,
        address=device.address,
        brand=guess_brand(name),
        rssi=getattr(adv, "rssi", None),
        manufacturer_data=dict(getattr(adv, "manufacturer_data", {}) or {}),
        service_uuids=list(getattr(adv, "service_uuids", []) or []),
    )


async def scan(timeout: float = 8.0, source: str = "auto") -> list[CubeDevice]:
    """扫描智能魔方。

    source: "auto" 匹配三品牌；"gan"/"moyu32"/"qiyi" 只保留对应品牌。
    返回按 RSSI 降序去重列表。
    """
    found: dict[str, CubeDevice] = {}

    def on_adv(device, adv):
        dev = _to_device(device, adv)
        if dev.brand == "unknown":
            return
        if source != "auto" and dev.brand != source:
            return
        found[dev.address] = dev

    scanner = BleakScanner(detection_callback=on_adv)
    await scanner.start()
    try:
        await asyncio.sleep(timeout)
    finally:
        await scanner.stop()

    devices = list(found.values())
    devices.sort(key=lambda d: (d.rssi if d.rssi is not None else -999), reverse=True)
    return devices


async def find_one(timeout: float = 10.0, source: str = "auto") -> CubeDevice | None:
    devices = await scan(timeout=timeout, source=source)
    return devices[0] if devices else None
