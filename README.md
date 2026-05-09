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
| GET | `/api/state` | 获取当前魔方状态 JSON |
| GET | `/api/facelets` | 获取 kociemba 格式面片字符串 |

## 相关项目

- [MOYU](../MOYU) - 基于 gan-cube-emu 的自动上分项目
