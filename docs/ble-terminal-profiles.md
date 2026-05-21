# BLE terminal profiles supported by RSTerm

RSTerm's BLE layer is designed as a transport that can expose one or more logical terminal ports over one BLE connection. This keeps traditional BLE UART devices compatible while allowing boards such as ESP32-C3/S3 to publish multiple UARTs, logs, shell channels, or control channels.

## 1. Classic BLE UART compatibility

RSTerm still supports a single terminal stream using a Notify/Indicate characteristic for device-to-host data and a Write/Write Without Response characteristic for host-to-device data.

Built-in service profiles:

| Profile | Service UUID | TX / Notify | RX / Write |
|---|---|---|---|
| Nordic UART Service | `6E400001-B5A3-F393-E0A9-E50E24DCCA9E` | `6E400003-B5A3-F393-E0A9-E50E24DCCA9E` | `6E400002-B5A3-F393-E0A9-E50E24DCCA9E` |
| TI / SimpleLink style | `FFF0` | `FFF1` | `FFF2` |
| HM-10 / JDY style | `FFE0` | `FFE1` | `FFE1` |
| Adafruit Bluefruit LE | `ADAFAF00-C464-4BDA-B3B2-2241514ADF3C` | `ADAFAF01-C464-4BDA-B3B2-2241514ADF3C` | `ADAFAF02-C464-4BDA-B3B2-2241514ADF3C` |
| DFBx style | `0000DFB0-0000-1000-8000-00805F9B34FB` | `0000DFB1-0000-1000-8000-00805F9B34FB` | `0000DFB2-0000-1000-8000-00805F9B34FB` |

If a known service is not found, RSTerm will try to auto-discover services that contain at least one Notify/Indicate characteristic and one Write/Write Without Response characteristic.

## 2. RSTerm multi-characteristic UART profile

This profile is simple and easy to debug with BLE scanner apps. Each logical port has its own TX/RX characteristic pair.

Service UUID:

```text
B7E40001-B5A3-F393-E0A9-E50E24DCCA9E
```

| Logical port | Name | Device -> RSTerm Notify | RSTerm -> Device Write |
|---:|---|---|---|
| 0 | UART0 | `B7E40002-B5A3-F393-E0A9-E50E24DCCA9E` | `B7E40003-B5A3-F393-E0A9-E50E24DCCA9E` |
| 1 | UART1 | `B7E40004-B5A3-F393-E0A9-E50E24DCCA9E` | `B7E40005-B5A3-F393-E0A9-E50E24DCCA9E` |

The current Rust implementation can also auto-pair more Notify/Write characteristics in one service, up to 8 logical ports. Known UUIDs should still be preferred for stable device UX.

## 3. RSTerm BLE MUX profile

The MUX profile uses only one Notify characteristic and one Write characteristic, then multiplexes arbitrary logical ports in a small binary frame. This is the recommended long-term profile for devices that need more than two streams.

Service UUID:

```text
7A6E0001-B5A3-F393-E0A9-E50E24DCCA9E
```

| Characteristic | UUID | Direction |
|---|---|---|
| MUX_RX | `7A6E0002-B5A3-F393-E0A9-E50E24DCCA9E` | RSTerm -> device Write / Write Without Response |
| MUX_TX | `7A6E0003-B5A3-F393-E0A9-E50E24DCCA9E` | Device -> RSTerm Notify / Indicate |

Frame format:

```text
byte 0      port
byte 1      frame_type
byte 2..3   payload length, little endian u16
byte 4..    payload
```

Currently implemented frame type:

```text
0x01 DATA
```

Recommended logical port kinds:

```text
0  main shell / UART0
1  UART1
2  device log
3  command/control channel
4  file transfer channel
5  GPIO/control channel
```

Future extensions can add frame types for port discovery, flow control, file transfer metadata, and structured device status without changing the BLE service layout.
