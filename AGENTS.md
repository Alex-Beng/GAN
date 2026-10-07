# GAN — Agent Guide

## Build & Flash

This repo is a **Cargo workspace** with three crates:

- `emu/` — GATT server firmware (`gan-emu` bin), the original emulator
- `proxy/` — GATT client firmware (`gan-proxy` bin), connects physical cubes
- `shared/` — lib crate shared by both (`crypto`, `protocol`, `device_config`)

**Per-crate sdkconfig**: `emu` and `proxy` need different BLE roles, and
`esp-idf-sys` is built once per invocation. The sdkconfig is selected via
per-crate `.cargo/config.toml` env vars (`ESP_IDF_SYS_ROOT_CRATE`,
`ESP_IDF_SDKCONFIG_DEFAULTS`) — so **build from inside the crate directory**:

- Build emu: `cd emu && cargo build -p emu`
- Build proxy: `cd proxy && cargo build -p proxy`
- Flash + monitor: `cd emu && cargo run -p emu` (or `espflash flash --monitor ... target/riscv32imc-esp-espidf/debug/gan-emu`)
- Requires nightly Rust (`rust-toolchain.toml`), `espflash` installed, and ESP-IDF v5.5.3 at `D:\esp` (set via `ESP_IDF_TOOLS_INSTALL_DIR` in root `.cargo/config.toml`).
- Building with plain `cargo build` from the workspace root does **not** select a sdkconfig — always build from the crate dir or with the env vars set.

## Project Structure

- Workspace root: `Cargo.toml` (`[workspace] members = ["shared","emu","proxy"]`)
- `emu/src/main.rs` — wires BLE, HTTP, WiFi, serial in parallel
- `emu/src/`: `ble.rs` (BLE peripheral, encrypt/notify), `http.rs` (ESP HTTP server + web UI), `wifi.rs` (STA/AP), `serial.rs` (USB JTAG console), `index.html` (embedded)
- `shared/src/`: `protocol.rs` (bit-packed message encoding for 6 cube protocols), `crypto.rs` (AES-CBC/ECB via ESP-IDF hw), `device_config.rs` (MACs, keys, BLE UUIDs per protocol)
- `proxy/src/main.rs` — self-contained GAN cube gateway: BLE Central (scan/connect/decrypt a physical GAN cube) + BLE Peripheral (virtual GAN cube GATT server)
- `proxy/src/server.rs` — virtual GAN Gen2 cube peripheral (advertise + state/cmd chars + notify)
- No Rust tests (`harness = false`)

## Protocols Supported

Gen2 (V1/GANi3SB), Gen3 (V2/GANicV2S), Gen4 (V3/GANi4), Gan251 (V3-2), MoYu32 (WCU), QiYi (Smart). All config in `device_config.rs`. Protocol switches at runtime; stored in NVS.

GAN device names must match CubeStation's `devicedata` model table so the app selects the right protocol version (V1/V2/V3). See `docs/cubestation-app-reverse.md` and `docs/cubestation-devicedata.json`.

## Testing

- **All tests are Python scripts** under `scripts/` — no Rust test infrastructure
- Dependencies: `requests`, `pyserial` (install via `pip` as needed)
- HTTP API tests: `scripts/test_api.py` (configure target IP as first arg)
- Cube command line: `scripts/cube_cmd.py` (send face moves via HTTP)
- CFOP/OLL solver tests: many `scripts/test_*.py` files
- Protocol regression over USB: `python scripts/proto_test.py` — builds, flashes via espflash, parses device IP from boot logs, runs per-protocol HTTP matrix (switch/reset/moves/scramble/invalid) and fails on panic/reboot markers in the serial log. Options: `--port COMx`, `--ip`, `--protocols gen2,moyu32`, `--quick`, `--skip-devices`. Expected values come from `scripts/cube_sim.py` (mirror of `protocol.rs` cube math — keep in sync).
- MoYu32 BLE conformance: `python scripts/ble_check.py` — bleak-based; validates advertising identity (name/MAC/CIC/service UUID) and the WCU wire format (161/163/164/165, crypto) against the csTimer reference. Requires `pip install bleak pycryptodome`.

## Multi-brand BLE proxy (PC-side)

`python scripts/proxy.py` — PC 端 BLE Central 代理：连接实体智能魔方（GAN v2/v3/v4、魔域 WCU_MY3、奇艺 QY-QYSC），解析转动并批量 `POST /api/moves` 到固件（由固件按 Web UI 选定的目标协议重新编码广播）。取代原先 MOYU 的 `tools/gan_proxy.py`。架构参考 `huizhiLLL/DCTimer-BLE`（`SmartCubeProtocol` 接口 + 每品牌协议类 + 扫描/分发）。子包 `scripts/proxy/`：`protocol.py`（接口/事件）、`scanner.py`（扫描+名字初筛）、`ble_client.py`（连接+GATT 服务 UUID 复核选协议）、`crypto.py`（GAN/魔域 AES-ECB+IV+%255 派生、奇艺 ECB+CRC16）、`gan.py`/`moyu32.py`/`qiyi.py`（各品牌解析）、`http_client.py`、`logger.py`。

- 用法：`python scripts/proxy.py --scan`（列设备）；`--source auto|gan|moyu32|qiyi --host <ESP_IP>`（转发）；`--csv`（CSV 落盘，**默认关闭**，写 `data/proxy/`）。
- 品牌识别：扫描按设备名初筛，**连接后按 GATT 服务 UUID 权威复核**（区分 GAN v2/v3/v4 等同品牌多协议），特征 UUID 兜底。
- 依赖：`pip install -r scripts/requirements-proxy.txt`（bleak/requests/pycryptodome）。

## Simulation & Debug

- Wokwi sim: `wokwi.toml` + `diagram.json` — controlled via VSCode extension `wokwi.wokwi-vscode`
- GDB: `riscv32-esp-elf-gdb` on `localhost:3333` (VSCode launch config included)
- Serial monitor: `espflash monitor -p COM3` or `python -m serial.tools.miniterm COM3 115200`
- ESP32-C3 exposes USB_SERIAL_JTAD (two COM ports; pick the higher-numbered UART port)

## SDK Config

`sdkconfig.defaults` is the source of truth: BLE NimBLE peripheral only, WiFi STA+AP, dynamic frequency scaling (160/40MHz), no light sleep, 8KB main task stack.

## Key Constraints

- `.cargo/config.toml` is **required** reading — sets target, linker, build-std, and env vars. Do not touch without understanding.
- BLE advertising must include Manufacturer Specific Data with MAC in last 6 bytes (see `gen2.md` for exact format).
- Encryption keys are derived from MAC + base key/IV in `crypto.rs:derive_key_iv`.
- HTTP API: `GET /`, `POST /api/move` (single), `POST /api/moves` (batch), `GET /api/state`, `GET /api/facelets`.
