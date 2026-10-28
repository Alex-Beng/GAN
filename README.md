# GAN

> **GAN** = **G**eneral-cube **A**dapter **N**exus

一个跑在 ESP32-C3 上的多品牌智能魔方网关。它不只做模拟：

- **emu 模式** —— 把自己伪装成一块真实的智能魔方，让官方 App 连上来。
- **proxy 模式** —— 同时连实体魔方和手机 App，把实体魔方的转动实时转发出去。

两种模式的最大亮点都是 **通过 HTTP 网页控制/配置**，不需要装额外客户端，也不需要重新烧录固件：浏览器打开设备 IP 就能转动魔方、切换协议、配 WiFi、看状态。

## 两种模式

| | emu（模拟） | proxy（代理/网关） |
|---|---|---|
| BLE 角色 | Peripheral（从机） | Central + Peripheral（双角色） |
| 数据来源 | 固件内部的虚拟魔方 | 真实魔方（扫描并自动识别） |
| 典型场景 | 没有实体魔方时让 App 正常工作、测试协议 | 把实体魔方桥接到任意品牌 App |
| 网页可控 | ✅ | ✅（含输出协议切换） |

### emu 模式

模拟一块 GAN 智能魔方，作为 BLE 外设广播；App 连接后即可正常收发。

- 内部维护完整的魔方状态机（cubie + facelet）
- 通过 HTTP 注入转动，实时推送给已连接的 App
- 支持协议：Gen2、Gen3、Gen4、Gan251、MoYu(WCU)、QiYi、Robot、Timer

### proxy 模式

单固件跑双 BLE 角色，是一个自包含的魔方网关：

- **Central**：扫描并连接实体魔方，按 GATT 服务 UUID 权威识别输入协议
  （GAN Gen2/Gen3/Gen4、MoYu WCU、MoYu MHC、QiYi），解密并解析出转动
- **Peripheral**：按网页里选择的**输出协议**广播成虚拟魔方，把转动转发给手机 App
- 输入协议自动识别，输出协议由用户在网页上选，与调用方无关

## 网页控制（核心亮点）

设备连上 WiFi（或自身开热点）后，浏览器访问设备 IP 即可得到控制台：

- 单步 / 批量转动、重置、清空队列
- 实时魔方状态（2D 展开图、cubie 状态、facelet 字符串）
- 协议切换（即时生效并持久化到 NVS）
- WiFi STA/AP 配置、扫描
- 芯片温度与频率监控

同一套网页 UI 由 emu 和 proxy 共用，proxy 里额外多一个输出协议选择。

## 常用 HTTP API

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | 网页控制台 |
| POST | `/api/move` | 单步转动 `{"face":0-5,"dir":0-1}` |
| POST | `/api/moves` | 批量转动 `[{"face":0,"dir":0},...]` |
| POST | `/api/reset` | 重置为复原状态 |
| GET | `/api/state` | 当前 cubie 状态 `{cp, co, ep, eo}` |
| GET | `/api/facelets` | kociemba 格式面片字符串 |
| GET/POST | `/api/protocol` | 读取 / 切换协议 |
| GET | `/api/wifi/status` · `/api/wifi/scan` · `/api/wifi/saved` | WiFi 状态与配置 |
| GET | `/api/temp` · `/api/freq` | 芯片温度 / 频率 |

## 硬件与构建

- ESP32-C3 开发板
- Rust nightly + `espflash`、ESP-IDF v5.5.3

两个模式各有独立的 sdkconfig，**必须进到对应 crate 目录里构建**：

```bash
# emu
cd emu && cargo build -p emu
cd emu && cargo run -p emu          # 烧录并开串口监视

# proxy
cd proxy && cargo build -p proxy
cd proxy && cargo run -p proxy
```

## 相关项目

- **MOYU** = **M**ore **O**nline **Y**our smart-c**U**be —— 自动上分项目。**未开源**。

## 串口日志调试

固件通过 UART 输出日志（115200），烧录后可这样查看：

```bash
# 推荐：espflash monitor（能解析 panic 堆栈）
espflash monitor -p COM3

# ESP-IDF 自带
idf.py -p COM3 monitor

# 通用串口工具
python -m serial.tools.miniterm COM3 115200
```

**查看 COM 端口**：设备管理器 → 端口(COM和LPT)，或命令行 `mode`。ESP32-C3 通常出现两个 COM 口，选数字较大的（UART 口）。

## License

[MIT](LICENSE) © 2026 Alex Beng
