"""HTTP 下发客户端：把转动批量 POST 到 gan-cube-emu 的 /api/moves。

对齐现有 `scripts/cube_cmd.py` 的 requests 用法。固件协议无关：只需 {face, dir}，
由固件按当前目标协议重新编码。
"""
from __future__ import annotations

import logging

import requests

logger = logging.getLogger(__name__)


class CubeHttpClient:
    def __init__(self, host: str, port: int = 80, timeout: float = 5.0):
        self.base_url = f"http://{host}:{port}"
        self.timeout = timeout
        self._session = requests.Session()

    def open(self) -> None:
        try:
            r = self._session.get(f"{self.base_url}/api/state", timeout=self.timeout)
            logger.info("ESP32 就绪: %s (%s)", self.base_url, r.status_code)
        except Exception as e:
            logger.warning("ESP32 探测失败 (%s): %s", self.base_url, e)

    def send_moves_raw(self, steps: list[dict]) -> bool:
        """批量下发 [{"face":0,"dir":0}, ...]。"""
        if not steps:
            return True
        try:
            r = self._session.post(
                f"{self.base_url}/api/moves", json=steps, timeout=self.timeout
            )
            logger.debug("下发 %d 步 → %s %s", len(steps), r.status_code, r.text.strip()[:120])
            return r.status_code == 200
        except Exception as e:
            logger.warning("下发失败 (%d 步): %s", len(steps), e)
            return False

    def set_delay(self, delay_ms: int) -> None:
        try:
            self._session.post(
                f"{self.base_url}/api/delay", json={"delay": delay_ms}, timeout=self.timeout
            )
            logger.info("已设置固件转动延迟: %dms", delay_ms)
        except Exception as e:
            logger.warning("设置延迟失败: %s", e)

    def close(self) -> None:
        self._session.close()
