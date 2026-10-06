"""CSV 记录（可选，默认关闭）。

沿用 MOYU `proxy_logger` 的行格式：timestamp,type,face,dir,qw..vz,facelets
输出目录：<仓库根>/data/proxy/
"""
from __future__ import annotations

import csv
import logging
from datetime import datetime
from pathlib import Path

logger = logging.getLogger(__name__)

_REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DIR = _REPO_ROOT / "data" / "proxy"

CSV_HEADER = [
    "timestamp", "type",
    "face", "dir",
    "qw", "qx", "qy", "qz", "vx", "vy", "vz",
    "facelets",
]


class ProxyLogger:
    def __init__(self, output_dir: str | Path | None = None):
        self.output_dir = Path(output_dir) if output_dir else DEFAULT_DIR
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self._fh = None
        self._writer = None
        self._row_count = 0
        self._path = None
        self._open_file()

    def _open_file(self) -> None:
        today = datetime.now().strftime("%Y%m%d")
        idx = 0
        while True:
            path = self.output_dir / f"{today}_{idx}.csv"
            if not path.exists():
                break
            idx += 1
        self._fh = open(path, "w", newline="", encoding="utf-8")
        self._writer = csv.writer(self._fh)
        self._writer.writerow(CSV_HEADER)
        self._path = path
        logger.info("CSV 日志: %s", path)

    def log_move(self, face: int, direction: int) -> None:
        ts = datetime.now().isoformat(timespec="microseconds")
        self._writer.writerow([ts, "move", face, direction, "", "", "", "", "", "", "", ""])
        self._maybe_flush()

    def log_gyro(self, x: float, y: float, z: float, w: float) -> None:
        ts = datetime.now().isoformat(timespec="microseconds")
        self._writer.writerow([ts, "gyro", "", "", f"{w:.6f}", f"{x:.6f}", f"{y:.6f}", f"{z:.6f}", "", "", "", ""])
        self._maybe_flush()

    def log_facelets(self, facelets: str) -> None:
        ts = datetime.now().isoformat(timespec="microseconds")
        self._writer.writerow([ts, "facelets", "", "", "", "", "", "", "", "", "", facelets])
        self._maybe_flush()

    def log_battery(self, percent: int) -> None:
        ts = datetime.now().isoformat(timespec="microseconds")
        self._writer.writerow([ts, "battery", "", percent, "", "", "", "", "", "", "", ""])
        self._maybe_flush()

    def _maybe_flush(self) -> None:
        self._row_count += 1
        if self._row_count % 50 == 0:
            self._fh.flush()

    def close(self) -> None:
        if self._fh:
            self._fh.flush()
            self._fh.close()
            self._fh = None
            logger.info("CSV 已关闭: %s (共 %d 行)", self._path, self._row_count)
