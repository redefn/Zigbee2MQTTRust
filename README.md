# Zigbee2MQTTRust

A lightweight [Zigbee2MQTT](https://www.zigbee2mqtt.io/) replacement written in Rust, built for the Raspberry Pi Zero
(ARMv6, 512 MB RAM) and the SMLIGHT SLZB-06 coordinator (CC2652P, Z-Stack ZNP, TCP port 6638 or USB). Goal: stay
compatible with the Zigbee2MQTT MQTT API so Home Assistant does not notice the swap.

Status: phase 0 (scaffold). The first target device is a Tuya two-channel dimmer.

## Layout

| Path | Contents |
| --- | --- |
| `crates/znp` | Z-Stack MT frames (`0xFE ... FCS` codec); transport and SREQ/SRSP correlation to follow |
| `crates/zcl` | ZCL frames and clusters, ZDO (phase 2) |
| `crates/devices` | device definitions and converters (phase 2) |
| `crates/bridge` | the `z2m-rs` binary, Z2M-compatible MQTT API (phase 3) |
| `tools/znp-tap` | TCP proxy that records ZNP frames between Z2M and the coordinator |
| `docker/` | reference Zigbee2MQTT + Mosquitto for debugging and comparison |

## Building for the Pi Zero

```sh
cargo install cross --locked
cross build --release --target arm-unknown-linux-musleabihf --workspace
scp target/arm-unknown-linux-musleabihf/release/z2m-rs pi@raspberrypi.local:
```

The Pi Zero and Zero W are ARMv6: an `armv7-*` target fails with "Illegal instruction". For a Pi Zero 2 W running a
64-bit OS use `aarch64-unknown-linux-musl`. CI builds the ARMv6 binary on every PR (artifact `pi-zero-armv6`).

## Reference Zigbee2MQTT and frame capture

The SLZB-06 usually accepts only one TCP client at a time, so run Z2M and `z2m-rs` alternately.
Back up `database.db` and the coordinator before every switch.

```sh
cargo run -p znp-tap -- --upstream <slzb-ip>:6638 --out capture.log
mkdir -p docker/z2m/data && cp docker/z2m/configuration.example.yaml docker/z2m/data/configuration.yaml
docker compose -f docker/docker-compose.yml up
```

Z2M frontend: http://localhost:8080. Every ZNP frame lands in `capture.log` with its direction and timestamp.
Captures will become tests replayed through a fake transport.

## Tests

```sh
cargo test --workspace
```
