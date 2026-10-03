# gan-cube-emu — Agent Guide

## Build & Flash

- Build: `cargo build` (uses `.cargo/config.toml` — target `riscv32imc-esp-espidf`, linker `ldproxy`, `build-std`)
- Flash + monitor: `cargo espflash flash --monitor --chip esp32c3`
- Runner already configured in `.cargo/config.toml` as `espflash flash --monitor`, so no extra flag needed after build.
- Requires nightly Rust (`rust-toolchain.toml`), `espflash` installed, and ESP-IDF v5.5.3 at `D:\esp` (set via `ESP_IDF_TOOLS_INSTALL_DIR`).

## Project Structure

- Single binary crate (`harness = false` — **no Rust tests**)
- Entrypoint: `src/main.rs` — wires BLE, HTTP, WiFi, serial in parallel
- Modules: `ble.rs` (BLE peripheral, encrypt/notify), `http.rs` (ESP HTTP server + web UI), `wifi.rs` (STA/AP), `serial.rs` (USB JTAG console), `protocol.rs` (bit-packed message encoding for 6 cube protocols), `crypto.rs` (AES-CBC/ECB via ESP-IDF hw), `device_config.rs` (MACs, keys, BLE UUIDs per protocol)
- Web UI: `src/index.html` (embedded at compile time)

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
