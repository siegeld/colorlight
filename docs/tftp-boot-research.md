# TFTP Boot: Hardcoded Server IP Research

## Current State

Both boot stages use a **hardcoded TFTP server IP of `10.11.6.65`**:

| Stage | What fetches | Where IP is set | Protocol |
|-------|-------------|-----------------|----------|
| BIOS | `boot.bin` (firmware) | `remote_ip` in `gateware/colorlight.py` — baked into bitstream | Raw UDP/TFTP |
| Firmware | `<mac>.yml` (layout config) | Hardcoded in `sw_rust/barsign_disp/src/main.rs` | smoltcp DHCP + TFTP |

Neither stage discovers the server dynamically. Changing the server requires rebuilding the bitstream (BIOS) or firmware.

## LiteX BIOS Source Analysis

The BIOS network stack is in [litex/soc/software/libliteeth/](https://github.com/enjoy-digital/litex/tree/master/litex/soc/software/libliteeth/) and [litex/soc/software/bios/boot.c](https://github.com/enjoy-digital/litex/tree/master/litex/soc/software/bios/boot.c). Key findings:

### TFTP client (`tftp.c`) does not validate source IP

The `rx_callback` receives `src_ip` as a parameter but **never checks it**. It only validates:
- Packet length (`length < 4`)
- Destination port (`dst_port != PORT_IN`)
- TFTP opcode and block number

This means the BIOS will accept a TFTP response from **any IP address**.

### UDP layer (`udp.c`) only checks destination IP

`process_udp()` verifies that the packet is addressed to the device's own IP (`my_ip`), but accepts packets from any source IP. No checksums are verified — it relies on Ethernet CRC only.

### TFTP port is overridable at build time

```c
#ifndef TFTP_SERVER_PORT
#define TFTP_SERVER_PORT 69
#endif
```

Passing `-DTFTP_SERVER_PORT=<port>` during BIOS compilation would change the port the BIOS sends its TFTP requests to.

### No DHCP or BOOTP

The BIOS has no dynamic IP discovery. IP addresses come from `LOCALIP1-4` and `REMOTEIP1-4` defines generated into `soc.h` at synthesis time. There is an `#ifdef ETH_DYNAMIC_IP` section with `set_local_ip()` / `set_remote_ip()` helpers, but no DHCP client to call them.

## Proposed Solutions

### 1. Broadcast Hack (BIOS)

Set `remote_ip="255.255.255.255"` in `gateware/colorlight.py`. The BIOS will broadcast its TFTP RRQ, and any TFTP server on the subnet will receive and respond. Since the BIOS doesn't check source IP, it will accept the response.

**Pros:** Zero server-side configuration. Any TFTP server works.

**Cons:** If multiple TFTP servers have `boot.bin`, all respond (race condition). Mitigate by combining with a custom `TFTP_SERVER_PORT` — only a server listening on that port responds.

**dnsmasq config for custom port:**
```
# Listen for TFTP on a non-standard port (e.g., 6969)
tftp-port-range=6969,6969
```

### 2. DHCP `siaddr` / Option 66 (Firmware)

The patched smoltcp already exposes the DHCP server IP as `Config.server_ip` (the `siaddr` field from the DHCP response). The firmware just needs to use this value instead of the hardcoded IP when fetching `<mac>.yml`.

DHCP option 66 (TFTP Server Name) is the standard alternative. Most DHCP servers support both.

**dnsmasq config:**
```
dhcp-boot=boot.bin,,10.11.6.65    # sets siaddr
# or
dhcp-option=66,"10.11.6.65"       # option 66
```

**Code change:** In `main.rs`, replace the hardcoded `Ipv4Address([10, 11, 6, 65])` with `config.server_ip` from the DHCP event.

### 3. Fix Flash Boot (Eliminate BIOS TFTP)

The BIOS TFTP fetch exists because flash boot is broken. `gateware/colorlight.py` defines the flash chip as `GD25Q16` but rev 8.2 boards use `W25Q32JV`. Fixing this would let the BIOS load firmware directly from SPI flash, eliminating the BIOS TFTP dependency entirely.

**Pros:** Simplest boot path. No TFTP server needed for firmware.

**Cons:** Firmware updates require reflashing. TFTP boot is still useful for development.

## Recommended Approach

Implement all three — they're complementary:

1. **Broadcast hack** for BIOS development boot (quick iteration without reflashing)
2. **DHCP siaddr** for firmware config fetch (standard, works on any network)
3. **Flash boot fix** for production deployment (no server dependency)

## Source References

- [libliteeth/tftp.c](https://github.com/enjoy-digital/litex/blob/master/litex/soc/software/libliteeth/tftp.c) — BIOS TFTP client
- [libliteeth/udp.c](https://github.com/enjoy-digital/litex/blob/master/litex/soc/software/libliteeth/udp.c) — BIOS UDP stack
- [bios/boot.c](https://github.com/enjoy-digital/litex/blob/master/litex/soc/software/bios/boot.c) — BIOS network boot entry point
- `gateware/colorlight.py:272` — `remote_ip` parameter
- `sw_rust/barsign_disp/src/main.rs:272` — Hardcoded firmware TFTP server IP
- `sw_rust/smoltcp-0.8.0/src/socket/dhcpv4.rs` — DHCP `Config.server_ip` (siaddr)
