# Zigbee2MQTTRust

Lekki zamiennik [Zigbee2MQTT](https://www.zigbee2mqtt.io/) w Rust, przeznaczony na Raspberry Pi Zero (ARMv6, 512 MB RAM),
dla koordynatora SMLIGHT SLZB-06 (CC2652P, Z-Stack ZNP, TCP 6638 lub USB). Cel: zgodność z API MQTT Zigbee2MQTT,
żeby Home Assistant nie zauważył podmiany.

Status: faza 0 (szkielet). Pierwsze docelowe urządzenie to dwukanałowy ściemniacz Tuya.

## Struktura

| Ścieżka | Zawartość |
| --- | --- |
| `crates/znp` | ramki Z-Stack MT (codec `0xFE ... FCS`), później transport i korelacja SREQ/SRSP |
| `crates/zcl` | ramki i klastry ZCL, ZDO (faza 2) |
| `crates/devices` | definicje urządzeń i konwertery (faza 2) |
| `crates/bridge` | plik binarny `z2m-rs`, API MQTT zgodne z Z2M (faza 3) |
| `tools/znp-tap` | proxy TCP, które nagrywa ramki ZNP między Z2M a koordynatorem |
| `docker/` | referencyjny Zigbee2MQTT + Mosquitto do debugowania i porównań |

## Budowanie dla Pi Zero

```sh
cargo install cross --locked
cross build --release --target arm-unknown-linux-musleabihf --workspace
scp target/arm-unknown-linux-musleabihf/release/z2m-rs pi@raspberrypi.local:
```

Pi Zero i Zero W to ARMv6: target `armv7-*` da błąd „Illegal instruction”. Dla Pi Zero 2 W z 64-bitowym systemem
użyj `aarch64-unknown-linux-musl`. CI buduje plik ARMv6 przy każdym PR (artefakt `pi-zero-armv6`).

## Referencyjny Zigbee2MQTT i nagrywanie ramek

SLZB-06 po TCP zwykle przyjmuje tylko jednego klienta naraz, więc Z2M i `z2m-rs` uruchamiaj naprzemiennie.
Przed każdym przełączeniem zrób kopię `database.db` i backup koordynatora.

```sh
cargo run -p znp-tap -- --upstream <ip-slzb>:6638 --out capture.log
mkdir -p docker/z2m/data && cp docker/z2m/configuration.example.yaml docker/z2m/data/configuration.yaml
docker compose -f docker/docker-compose.yml up
```

Frontend Z2M: http://localhost:8080. Każda ramka ZNP trafia do `capture.log` z kierunkiem i czasem.
Nagrania staną się testami odtwarzanymi przez fałszywy transport.

## Testy

```sh
cargo test --workspace
```
