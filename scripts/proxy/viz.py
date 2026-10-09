"""运行时 3D 朝向可视化（OpenCV）。

品牌无关：只消费统一事件流里的已归一化四元数（GyroEvent）与 54 字符
面片串（StateEvent.facelets），不依赖任何具体协议解析。

渲染几何移植自 MOYU `tools/gan_proxy.py`（`_render_cube` 及其常量）。

依赖：opencv-python numpy（仅 --viz 路径需要）。
若未安装，`VizWindow` 构造会抛 ImportError，由调用方捕获并降级。
"""
from __future__ import annotations

import threading

import cv2
import numpy as np

WINDOW_TITLE = "智能魔方朝向"


def _quat_to_rotm(qw, qx, qy, qz):
    x2, y2, z2 = qx * qx, qy * qy, qz * qz
    xy, xz, yz = qx * qy, qx * qz, qy * qz
    wx, wy, wz = qw * qx, qw * qy, qw * qz
    return np.array([
        [1 - 2 * (y2 + z2), 2 * (xy - wz), 2 * (xz + wy)],
        [2 * (xy + wz), 1 - 2 * (x2 + z2), 2 * (yz - wx)],
        [2 * (xz - wy), 2 * (yz + wx), 1 - 2 * (x2 + y2)],
    ])


_CUBE_VERTS = np.array([
    [-1, -1, -1], [1, -1, -1], [1, 1, -1], [-1, 1, -1],
    [-1, -1,  1], [1, -1,  1], [1, 1,  1], [-1, 1,  1],
], dtype=np.float32)

_CUBE_EDGES = [
    (0, 1), (1, 2), (2, 3), (3, 0),
    (4, 5), (5, 6), (6, 7), (7, 4),
    (0, 4), (1, 5), (2, 6), (3, 7),
]

# 6 个面（顶点索引）+ 外法线 + 标准 GAN 配色（BGR）
# 约定 X→R, Y→F, Z→U
_CUBE_FACES = [
    (4, 5, 6, 7),   # +Z → U（上，白）
    (0, 3, 2, 1),   # -Z → D（下，黄）
    (2, 6, 7, 3),   # +Y → B（后，蓝）
    (0, 1, 5, 4),   # -Y → F（前，绿）
    (1, 2, 6, 5),   # +X → R（右，红）
    (0, 4, 7, 3),   # -X → L（左，橙）
]
_FACE_NORMALS = [
    (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0), (1, 0, 0), (-1, 0, 0),
]
_FACE_COLORS = [
    (255, 255, 255),  # U 白
    (0, 255, 255),    # D 黄
    (255, 0, 0),      # B 蓝
    (0, 255, 0),      # F 绿
    (0, 0, 255),      # R 红
    (0, 165, 255),    # L 橙
]
_EDGE_COLOR = (30, 30, 30)

# 每个面(Kociemba 顺序)在 54 面片串中的起始偏移：U,D,B,F,R,L
_FACELET_OFFSET = [0, 27, 45, 18, 9, 36]
_FACELET_BGR = {
    "U": (255, 255, 255), "R": (0, 0, 255), "F": (0, 255, 0),
    "D": (0, 255, 255), "L": (0, 165, 255), "B": (255, 0, 0),
}
# 每面的贴纸网格局部基（origin=tangent中心, row=行方向, col=列方向）
_FACE_GRID = [
    ((0, 0, 1), (0, -1, 0), (1, 0, 0)),     # U +Z
    ((0, 0, -1), (0, 1, 0), (1, 0, 0)),     # D -Z
    ((0, 1, 0), (0, 0, -1), (-1, 0, 0)),    # B +Y
    ((0, -1, 0), (0, 0, -1), (1, 0, 0)),    # F -Y
    ((1, 0, 0), (0, 0, -1), (0, 1, 0)),     # R +X
    ((-1, 0, 0), (0, 0, -1), (0, -1, 0)),   # L -X
]

_STICKER = 2.0 / 3.0


def _build_face_cells():
    """为每个面生成 3x3 贴纸网格的局部 3D 角点 [6, 9, 4, 3]"""
    cells = []
    for origin, row, col in _FACE_GRID:
        o = np.array(origin, dtype=np.float32)
        r = np.array(row, dtype=np.float32)
        c = np.array(col, dtype=np.float32)
        face_cells = []
        for rc in range(3):
            for cc in range(3):
                center = o + r * (rc - 1) * _STICKER + c * (cc - 1) * _STICKER
                quad = np.array([
                    center + r * (-_STICKER/2) + c * (_STICKER/2),
                    center + r * (-_STICKER/2) + c * (-_STICKER/2),
                    center + r * (_STICKER/2) + c * (-_STICKER/2),
                    center + r * (_STICKER/2) + c * (_STICKER/2),
                ], dtype=np.float32)
                face_cells.append(quad)
        cells.append(face_cells)
    return np.array(cells)


_FACE_CELLS = _build_face_cells()


def _render_cube(qw, qx, qy, qz, size=400, facelets=None, view_yaw=0.0):
    R = _quat_to_rotm(qw, qx, qy, qz)
    # 视角旋转：世界 Z 轴 → 屏幕 Y 轴方向（重力朝下）
    R_view = np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0]], dtype=np.float32)
    cyaw, syaw = np.cos(view_yaw), np.sin(view_yaw)
    R_yaw = np.array([[cyaw, 0, syaw], [0, 1, 0], [-syaw, 0, cyaw]], dtype=np.float32)
    verts3d = (((_CUBE_VERTS @ R.T) @ R_view.T) @ R_yaw.T) * (size * 0.3)
    cx, cy = size // 2, size // 2
    img = np.zeros((size, size, 3), dtype=np.uint8)

    def proj(v):
        # X 翻转修正左手镜像；Y 直接映射（重力朝下）
        return np.round(np.stack([cx - v[:, 0], cy + v[:, 1]], axis=1)).astype(int)

    # 面法线变换到视角坐标系（含 yaw）
    normals_view = [
        ((R_view @ (R @ np.array(n, dtype=np.float32))) @ R_yaw.T)
        for n in _FACE_NORMALS
    ]

    # 变换贴纸网格角点到视角坐标
    cells = (_FACE_CELLS @ R.T) @ R_view.T @ R_yaw.T

    for fi, (face, color, n_view) in enumerate(zip(_CUBE_FACES, _FACE_COLORS, normals_view)):
        nz = n_view[2]
        if nz <= 0:
            continue
        shade = 0.3 + 0.7 * nz
        if facelets is not None:
            off = _FACELET_OFFSET[fi]
            for cell_idx in range(9):
                ch = facelets[off + cell_idx]
                rgb = _FACELET_BGR.get(ch)
                if rgb is None:
                    continue
                quad = cells[fi][cell_idx] * (size * 0.3)
                pts = proj(quad)
                cv2.fillPoly(img, [pts], tuple(int(c * shade) for c in rgb))
        else:
            face_pts = proj(verts3d[list(face)])
            cv2.fillPoly(img, [face_pts], tuple(int(c * shade) for c in color))

    # 边缘叠加
    pts = proj(verts3d)
    for (i, j) in _CUBE_EDGES:
        cv2.line(img, tuple(pts[i]), tuple(pts[j]), _EDGE_COLOR, 1)

    # 固定重力箭头（始终朝下）
    g_len = size * 0.35
    g_end = np.array([cx, cy + g_len]).astype(int)
    g_start = np.array([cx, cy]).astype(int)
    cv2.arrowedLine(img, tuple(g_start), tuple(g_end), (0, 0, 255), 2, tipLength=0.2)
    cv2.putText(img, f"qw={qw:.3f} qx={qx:.3f}", (8, 20),
                cv2.FONT_HERSHEY_SIMPLEX, 0.45, (100, 200, 100), 1)
    cv2.putText(img, f"qy={qy:.3f} qz={qz:.3f}", (8, 36),
                cv2.FONT_HERSHEY_SIMPLEX, 0.45, (100, 200, 100), 1)
    return img


class VizWindow:
    """独立线程里渲染 3D 朝向窗口；事件线程只更新共享状态。"""

    def __init__(self, size: int = 400):
        self._size = size
        self._latest_quat: tuple[float, float, float, float] | None = None
        self._latest_facelets: str | None = None
        self._view_yaw = 0.0
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def set_quat(self, x: float, y: float, z: float, w: float) -> None:
        with self._lock:
            self._latest_quat = (w, x, y, z)

    def set_facelets(self, facelets: str) -> None:
        with self._lock:
            self._latest_facelets = facelets

    def start(self) -> None:
        self._stop.clear()
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
        cv2.destroyAllWindows()

    def _loop(self) -> None:
        cv2.namedWindow(WINDOW_TITLE, cv2.WINDOW_NORMAL)
        cv2.resizeWindow(WINDOW_TITLE, self._size, self._size)
        default_quat = (1.0, 0.0, 0.0, 0.0)
        while not self._stop.is_set():
            with self._lock:
                q = self._latest_quat if self._latest_quat is not None else default_quat
                facelets = self._latest_facelets
                yaw = self._view_yaw
            img = _render_cube(*q, size=self._size, facelets=facelets, view_yaw=yaw)
            cv2.imshow(WINDOW_TITLE, img)
            key = cv2.waitKeyEx(30)
            if key == ord("q") or key == 27:
                self._stop.set()
                break
            elif key == 2555904:   # → 右
                self._view_yaw += 0.15
            elif key == 2424832:   # ← 左
                self._view_yaw -= 0.15
