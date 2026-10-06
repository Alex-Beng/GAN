"""GAN / 魔域 同款 AES-128 加解密。

参考 DCTimer-BLE `GanCubeCipher` / `Moyu32Cipher`（与 GAN `crypto.rs`
逐字节一致）：

    key[i] = (base[i] + mac[5-i]) % 255   (仅前 6 字节, MAC 逆序作盐)
    encode: 首块 XOR IV → AES-ECB；若多块, 末块 XOR IV → AES-ECB
    decode: 先解末块(多块时), 再解首块；每块解密后 XOR IV

注意：单块帧(20 字节)首末块重叠属正常，与固件实现一致。
"""
from __future__ import annotations

from Crypto.Cipher import AES

# GAN v2/v3/v4 根密钥/IV (ver=0, 普通型号)
GAN_KEY_BASE = bytes([
    0x01, 0x02, 0x42, 0x28, 0x31, 0x91, 0x16, 0x07,
    0x20, 0x05, 0x18, 0x54, 0x42, 0x11, 0x12, 0x53,
])
GAN_IV_BASE = bytes([
    0x11, 0x03, 0x32, 0x28, 0x21, 0x01, 0x76, 0x27,
    0x20, 0x95, 0x78, 0x14, 0x32, 0x12, 0x02, 0x43,
])
# GAN v2 AiCube 前缀型号
AICUBE_KEY_BASE = bytes([
    0x05, 0x12, 0x02, 0x45, 0x02, 0x01, 0x29, 0x56,
    0x12, 0x78, 0x12, 0x76, 0x81, 0x01, 0x08, 0x03,
])
AICUBE_IV_BASE = bytes([
    0x01, 0x44, 0x28, 0x06, 0x86, 0x21, 0x22, 0x28,
    0x51, 0x05, 0x08, 0x31, 0x82, 0x02, 0x21, 0x06,
])
# 魔域 WCU_MY3 根密钥/IV (与 GAN 同派生方案)
MOYU32_KEY_BASE = bytes([
    21, 119, 58, 92, 103, 14, 45, 31,
    23, 103, 42, 19, 155, 103, 82, 87,
])
MOYU32_IV_BASE = bytes([
    17, 35, 38, 37, 134, 42, 44, 59,
    85, 6, 127, 49, 126, 103, 33, 87,
])


def parse_mac(mac: str) -> bytes:
    """"AA:BB:CC:DD:EE:FF" → 6 字节（显示序）。"""
    parts = mac.strip().upper().split(":")
    if len(parts) != 6:
        raise ValueError(f"invalid mac: {mac}")
    return bytes(int(p, 16) for p in parts)


class BaseCipher:
    """AES-ECB + IV 的首末块加解密（GAN / 魔域 通用）。"""

    def __init__(self, base_key: bytes, base_iv: bytes, mac: str):
        mac_bytes = parse_mac(mac)
        key = bytearray(base_key)
        iv = bytearray(base_iv)
        for i in range(6):
            key[i] = (key[i] + mac_bytes[5 - i]) % 255
            iv[i] = (iv[i] + mac_bytes[5 - i]) % 255
        self.key = bytes(key)
        self.iv = bytes(iv)
        self._enc = AES.new(self.key, AES.MODE_ECB)
        self._dec = AES.new(self.key, AES.MODE_ECB)

    def encode(self, value: bytes) -> bytes:
        ret = bytearray(value)
        if len(ret) < 16:
            return bytes(ret)
        blk = bytes(a ^ b for a, b in zip(ret[:16], self.iv))
        ret[:16] = self._enc.encrypt(blk)
        if len(ret) > 16:
            off = len(ret) - 16
            blk = bytes(a ^ b for a, b in zip(ret[off:off + 16], self.iv))
            ret[off:off + 16] = self._enc.encrypt(blk)
        return bytes(ret)

    def decode(self, value: bytes) -> bytes:
        ret = bytearray(value)
        if len(ret) < 16:
            return bytes(ret)
        if len(ret) > 16:
            off = len(ret) - 16
            blk = self._dec.decrypt(bytes(ret[off:off + 16]))
            ret[off:off + 16] = bytes(a ^ b for a, b in zip(blk, self.iv))
        blk = self._dec.decrypt(bytes(ret[:16]))
        ret[:16] = bytes(a ^ b for a, b in zip(blk, self.iv))
        return bytes(ret)


class GanCipher(BaseCipher):
    def __init__(self, mac: str, ai_cube: bool = False):
        if ai_cube:
            super().__init__(AICUBE_KEY_BASE, AICUBE_IV_BASE, mac)
        else:
            super().__init__(GAN_KEY_BASE, GAN_IV_BASE, mac)


class Moyu32Cipher(BaseCipher):
    def __init__(self, mac: str):
        super().__init__(MOYU32_KEY_BASE, MOYU32_IV_BASE, mac)


# 奇艺: 纯 AES-128-ECB 固定密钥 (无 MAC 派生)
QIYI_AES_KEY = bytes([
    87, 177, 249, 171, 205, 90, 232, 167,
    156, 185, 140, 231, 87, 140, 81, 8,
])


class QiyiCipher:
    """奇艺 AES-128-ECB（16 字节对齐整块加解密）。"""

    def __init__(self):
        self._enc = AES.new(QIYI_AES_KEY, AES.MODE_ECB)
        self._dec = AES.new(QIYI_AES_KEY, AES.MODE_ECB)

    def encrypt(self, value: bytes) -> bytes:
        out = bytearray()
        for i in range(0, len(value), 16):
            out += self._enc.encrypt(bytes(value[i:i + 16]))
        return bytes(out)

    def decrypt(self, value: bytes) -> bytes:
        out = bytearray()
        for i in range(0, len(value), 16):
            out += self._dec.decrypt(bytes(value[i:i + 16]))
        return bytes(out)


def crc16_modbus(data: bytes, length: int = 0) -> int:
    """CRC16-MODBUS (poly 0xA001, init 0xFFFF)，与 QiyiCubeProtocol 一致。"""
    if length <= 0:
        length = len(data)
    crc = 0xFFFF
    for i in range(length):
        crc ^= data[i] & 0xFF
        for _ in range(8):
            if crc & 1:
                crc = (crc >> 1) ^ 0xA001
            else:
                crc >>= 1
    return crc & 0xFFFF
