#!/usr/bin/env python3
"""智能魔方 BLE 代理：连接实体魔方（GAN / 魔域 / 奇艺），解析转动并通过 HTTP
下发到 gan-cube-emu 固件（由固件按 Web UI 选择的目标协议广播）。

架构参考 huizhiLLL/DCTimer-BLE：扫描按名字初筛 → 连接后按 GATT 服务 UUID 复核
具体协议 → 各品牌协议类解析 → 统一 MoveEvent → POST /api/moves。

用法：
    python scripts/proxy.py --scan                              # 扫描列出设备
    python scripts/proxy.py --source auto --host 192.168.4.1    # 连接并转发（不落盘）
    python scripts/proxy.py --source gan --address AA:BB:CC:DD:EE:FF --csv
    python scripts/proxy.py --source auto --host 192.168.4.1 --verbose

依赖：bleak requests pycryptodome  (pip install -r scripts/requirements-proxy.txt)
"""
from __future__ import annotations

import argparse
import asyncio
import logging
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from proxy import scanner  # noqa: E402
from proxy.ble_client import CubeConnection  # noqa: E402
from proxy.http_client import CubeHttpClient  # noqa: E402
from proxy.protocol import (  # noqa: E402
    BatteryEvent,
    GyroEvent,
    MoveEvent,
    StateEvent,
)

logger = logging.getLogger("proxy")

DEBOUNCE_SEC = 0.1


def _format_ts() -> str:
    return time.strftime("%H:%M:%S") + f".{int(time.time() * 1000) % 1000:03d}"


class Proxy:
    def __init__(self, args):
        self.args = args
        self.http = None
        self.logger = None
        self._move_buffer: list[dict] = []
        self._debounce_timer = 0.0
        self._running = False

    def _on_event(self, event) -> None:
        if isinstance(event, MoveEvent):
            names = "URFDLB"
            dstr = "" if event.dir == 0 else "'"
            print(f"[{_format_ts()}] 转动: {names[event.face]}{dstr}  (face={event.face}, dir={event.dir})")
            if self.logger:
                self.logger.log_move(event.face, event.dir)
            self._move_buffer.append({"face": event.face, "dir": event.dir})
            self._debounce_timer = asyncio.get_event_loop().time() + DEBOUNCE_SEC
        elif isinstance(event, StateEvent):
            if self.args.verbose:
                print(f"[{_format_ts()}] 面片: {event.facelets}")
            if self.logger:
                self.logger.log_facelets(event.facelets)
        elif isinstance(event, GyroEvent):
            if self.logger:
                self.logger.log_gyro(event.x, event.y, event.z, event.w)
        elif isinstance(event, BatteryEvent):
            if self.args.verbose:
                print(f"[{_format_ts()}] 电量: {event.percent}%")
            if self.logger:
                self.logger.log_battery(event.percent)

    async def _flush_loop(self) -> None:
        while self._running:
            loop = asyncio.get_event_loop()
            remaining = (
                self._debounce_timer - loop.time()
                if self._debounce_timer > 0
                else DEBOUNCE_SEC
            )
            await asyncio.sleep(max(0.01, remaining))
            if self._debounce_timer > 0 and loop.time() >= self._debounce_timer:
                self._debounce_timer = 0.0
                await self._flush()

    async def _flush(self) -> None:
        if not self._move_buffer:
            return
        batch = self._move_buffer[:]
        self._move_buffer.clear()
        if self.http:
            await asyncio.to_thread(self.http.send_moves_raw, batch)

    async def run(self) -> None:
        if self.args.csv:
            from proxy.logger import ProxyLogger
            self.logger = ProxyLogger()

        device = None
        if self.args.address:
            device = scanner.CubeDevice(
                name=self.args.name or "",
                address=self.args.address,
                brand=self.args.source if self.args.source != "auto" else "unknown",
            )
        else:
            print(f"[{_format_ts()}] 扫描智能魔方 (source={self.args.source})...")
            device = await scanner.find_one(timeout=self.args.scan_timeout, source=self.args.source)
            if device is None:
                print(f"[{_format_ts()}] 未发现匹配的智能魔方")
                return
        print(f"[{_format_ts()}] 目标: {device.name or '(unknown)'} {device.address} [{device.brand}]")

        if self.args.host:
            self.http = CubeHttpClient(self.args.host, self.args.port)
            await asyncio.to_thread(self.http.open)
            print(f"[{_format_ts()}] 转发已启用: {self.args.host}:{self.args.port}")

        conn = CubeConnection(device, self._on_event, source=self.args.source)
        self._running = True
        flush_task = asyncio.create_task(self._flush_loop())
        try:
            ok = await conn.connect()
            if not ok:
                return
            print(f"[{_format_ts()}] 已连接，Ctrl+C 退出")
            while conn.connected:
                await asyncio.sleep(0.5)
        except asyncio.CancelledError:
            pass
        finally:
            self._running = False
            flush_task.cancel()
            await self._flush()
            await conn.disconnect()
            if self.http:
                self.http.close()
            if self.logger:
                self.logger.close()


async def _do_scan(source: str, timeout: float) -> None:
    print(f"扫描智能魔方 (source={source}, timeout={timeout}s)...\n")
    devices = await scanner.scan(timeout=timeout, source=source)
    if not devices:
        print("未发现匹配设备。")
        return
    for d in devices:
        rssi = f"{d.rssi} dBm" if d.rssi is not None else "?"
        print(f"  [{d.brand:>7}] {d.name or '(unknown)':<24} {d.address}  RSSI={rssi}")
        if d.service_uuids:
            print(f"            服务: {', '.join(d.service_uuids)}")


def main() -> None:
    parser = argparse.ArgumentParser(description="智能魔方 BLE 代理 (GAN/魔域/奇艺 → gan-cube-emu)")
    parser.add_argument("--source", default="auto", choices=["auto", "gan", "moyu32", "qiyi"],
                        help="来源品牌；默认 auto 自动识别")
    parser.add_argument("--address", default=None, help="指定 BLE 地址，跳过扫描")
    parser.add_argument("--name", default=None, help="配合 --address 时提供设备名（辅助品牌识别）")
    parser.add_argument("--host", default=None, help="ESP32 IP，提供后启用 HTTP 转发")
    parser.add_argument("--port", type=int, default=80, help="ESP32 端口 (默认 80)")
    parser.add_argument("--scan-timeout", type=float, default=8.0, help="扫描超时秒数")
    parser.add_argument("--scan", action="store_true", help="仅扫描列出设备")
    parser.add_argument("--csv", action="store_true", help="启用 CSV 落盘 (默认关闭)")
    parser.add_argument("--verbose", action="store_true", help="显示面片/电量等调试信息")
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    if args.scan:
        asyncio.run(_do_scan(args.source, args.scan_timeout))
        return

    proxy = Proxy(args)
    try:
        asyncio.run(proxy.run())
    except KeyboardInterrupt:
        print("\n用户中断")
    finally:
        print("已退出")


if __name__ == "__main__":
    main()
