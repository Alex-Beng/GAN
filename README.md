# gan-cube-emu

ESP32-C3 BLE 魔方式模拟器，模拟 GAN 智能魔方的通信协议，通过 HTTP API 控制转动。

## 功能

- 模拟 GANi3SB 魔方 BLE 外设
- 通过 HTTP API 控制魔方转动
- 支持 WiFi STA/AP 模式
- 兼容 GAN 官方 App（CubeStation）

## 硬件要求

- ESP32-C3 开发板
- Rust + espflash 工具链

## 使用

```bash
cargo espflash flash --monitor --chip esp32c3
```

## HTTP API

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Web 控制界面 |
| POST | `/api/move` | 单步转动 `{"face":0-5,"dir":0-1}` |
| POST | `/api/moves` | 批量转动 `[{"face":0,"dir":0},...]` |
| GET | `/api/state` | 获取当前cubie魔方状态 JSON `{cp: {}, co: {}, ep: {}, eo: {}}` |
| GET | `/api/facelets` | 获取 kociemba 格式面片字符串 |

## 相关项目6

- [MOYU](../MOYU) - 基于 gan-cube-emu 的自动上分项目

## 串口日志调试

固件通过 UART 输出运行日志（波特率 115200），烧录后可通过以下方式查看：

```bash
# 推荐：espflash monitor（能解析 panic 堆栈）
espflash monitor -p COM3

# ESP-IDF 自带
idf.py -p COM3 monitor

# 通用串口工具
python -m serial.tools.miniterm COM3 115200
# 或 PuTTY：Connection type → Serial，Serial line → COM3，Speed → 115200
```

**查看 COM 端口**：设备管理器 → 端口(COM和LPT)，或命令行 `mode`。ESP32-C3 通常出现两个 COM 口，选数字较大的（UART 口）。