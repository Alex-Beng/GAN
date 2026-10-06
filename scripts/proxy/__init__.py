"""BLE 智能魔方代理子包。

以 BLE Central 连接实体智能魔方（GAN / 魔域 / 奇艺），解析转动并统一转为
`MoveEvent(face, dir)`，再经 HTTP `POST /api/moves` 下发到 GAN 固件，
由固件按 Web UI 选择的目标协议重新编码广播。

架构参考 huizhiLLL/DCTimer-BLE：
    - protocol.SmartCubeProtocol  统一协议接口
    - scanner                     扫描 + 名字初筛
    - ble_client                  连接 / GATT 发现 / UUID 复核 / 订阅 / 分发
    - gan / moyu32 / qiyi         各品牌协议解析
    - crypto                      GAN / 魔域 同款 AES-ECB + IV + %255 派生
"""

from .protocol import (
    SmartCubeProtocol,
    MoveEvent,
    StateEvent,
    GyroEvent,
    BatteryEvent,
)

__all__ = [
    "SmartCubeProtocol",
    "MoveEvent",
    "StateEvent",
    "GyroEvent",
    "BatteryEvent",
]
