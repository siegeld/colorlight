#!/usr/bin/env python3
"""
Generate LiteEth UDP core for Colorlight 5A-75E with RTL8211FD PHY.
Simpler direct generation without full SoC.
"""

import os
import sys

from migen import *
from migen.fhdl import verilog

from liteeth.phy.ecp5rgmii import LiteEthPHYRGMII
from liteeth.mac import LiteEthMAC
from liteeth.core.arp import LiteEthARP
from liteeth.core.ip import LiteEthIP
from liteeth.core.udp import LiteEthUDP
from liteeth.core.icmp import LiteEthICMP

# Configuration
MAC_ADDRESS = 0x10E2D5000001
IP_ADDRESS  = 0x0A0B06FA  # 10.11.6.250
UDP_PORT    = 6000
CLK_FREQ    = 125000000

# RGMII delays (in seconds)
# RTL8211FD typically needs ~2ns RX delay from FPGA side
RX_DELAY = 2.0e-9
TX_DELAY = 0.0e-9


class LiteEthCore(Module):
    def __init__(self):
        # -------------------------------------------------------------------------
        # Clock and Reset
        # -------------------------------------------------------------------------
        self.sys_clock = Signal()
        self.sys_reset = Signal()

        # -------------------------------------------------------------------------
        # RGMII Interface
        # -------------------------------------------------------------------------
        self.rgmii_eth_clocks_tx = Signal()
        self.rgmii_eth_clocks_rx = Signal()
        self.rgmii_eth_rst_n = Signal()
        self.rgmii_eth_int_n = Signal()
        self.rgmii_eth_mdio = Signal()
        self.rgmii_eth_mdc = Signal()
        self.rgmii_eth_rx_ctl = Signal()
        self.rgmii_eth_rx_data = Signal(4)
        self.rgmii_eth_tx_ctl = Signal()
        self.rgmii_eth_tx_data = Signal(4)

        # -------------------------------------------------------------------------
        # UDP Interface (directly exposed)
        # -------------------------------------------------------------------------
        # UDP Source (received packets)
        self.udp_source_valid = Signal()
        self.udp_source_last = Signal()
        self.udp_source_ready = Signal()
        self.udp_source_src_port = Signal(16)
        self.udp_source_dst_port = Signal(16)
        self.udp_source_ip_address = Signal(32)
        self.udp_source_length = Signal(16)
        self.udp_source_data = Signal(32)
        self.udp_source_error = Signal(4)

        # UDP Sink (transmit packets)
        self.udp_sink_valid = Signal()
        self.udp_sink_last = Signal()
        self.udp_sink_ready = Signal()
        self.udp_sink_src_port = Signal(16)
        self.udp_sink_dst_port = Signal(16)
        self.udp_sink_ip_address = Signal(32)
        self.udp_sink_length = Signal(16)
        self.udp_sink_data = Signal(32)
        self.udp_sink_error = Signal(4)

        # -------------------------------------------------------------------------
        # Clock Domains
        # -------------------------------------------------------------------------
        self.clock_domains.cd_sys = ClockDomain("sys")
        self.clock_domains.cd_eth_rx = ClockDomain("eth_rx")
        self.clock_domains.cd_eth_tx = ClockDomain("eth_tx")

        self.comb += self.cd_sys.clk.eq(self.sys_clock)
        self.comb += self.cd_sys.rst.eq(self.sys_reset)

        # -------------------------------------------------------------------------
        # PHY
        # -------------------------------------------------------------------------
        # Create clock and data pad records
        class ClockPads:
            def __init__(self, tx, rx):
                self.tx = tx
                self.rx = rx

        class DataPads:
            def __init__(self):
                self.rst_n = Signal()
                self.int_n = Signal()
                self.mdio = Signal()
                self.mdc = Signal()
                self.rx_ctl = Signal()
                self.rx_data = Signal(4)
                self.tx_ctl = Signal()
                self.tx_data = Signal(4)

        clock_pads = ClockPads(self.rgmii_eth_clocks_tx, self.rgmii_eth_clocks_rx)
        data_pads = DataPads()

        # Connect external signals to pads
        self.comb += [
            data_pads.rst_n.eq(self.rgmii_eth_rst_n),
            data_pads.rx_ctl.eq(self.rgmii_eth_rx_ctl),
            data_pads.rx_data.eq(self.rgmii_eth_rx_data),
        ]
        self.comb += [
            self.rgmii_eth_tx_ctl.eq(data_pads.tx_ctl),
            self.rgmii_eth_tx_data.eq(data_pads.tx_data),
            self.rgmii_eth_mdc.eq(data_pads.mdc),
        ]
        # MDIO is bidirectional - handled by PHY

        self.submodules.ethphy = ethphy = LiteEthPHYRGMII(
            clock_pads = clock_pads,
            pads       = data_pads,
            tx_delay   = TX_DELAY,
            rx_delay   = RX_DELAY,
        )

        # Connect PHY clock domains
        self.comb += [
            self.cd_eth_rx.clk.eq(ethphy.crg.cd_eth_rx.clk),
            self.cd_eth_rx.rst.eq(ethphy.crg.cd_eth_rx.rst),
            self.cd_eth_tx.clk.eq(ethphy.crg.cd_eth_tx.clk),
            self.cd_eth_tx.rst.eq(ethphy.crg.cd_eth_tx.rst),
        ]

        # -------------------------------------------------------------------------
        # MAC
        # -------------------------------------------------------------------------
        self.submodules.ethmac = ethmac = LiteEthMAC(
            phy        = ethphy,
            dw         = 32,
            interface  = "crossbar",
            with_preamble_crc = True,
        )

        # -------------------------------------------------------------------------
        # ARP
        # -------------------------------------------------------------------------
        self.submodules.arp = arp = LiteEthARP(
            mac        = ethmac,
            mac_address = MAC_ADDRESS,
            ip_address  = IP_ADDRESS,
            clk_freq    = CLK_FREQ,
        )

        # -------------------------------------------------------------------------
        # IP
        # -------------------------------------------------------------------------
        self.submodules.ip = ip = LiteEthIP(
            mac        = ethmac,
            mac_address = MAC_ADDRESS,
            ip_address  = IP_ADDRESS,
            arp_table   = arp.table,
        )

        # -------------------------------------------------------------------------
        # ICMP (for ping)
        # -------------------------------------------------------------------------
        self.submodules.icmp = icmp = LiteEthICMP(
            ip         = ip,
            ip_address = IP_ADDRESS,
        )

        # -------------------------------------------------------------------------
        # UDP
        # -------------------------------------------------------------------------
        self.submodules.udp = udp = LiteEthUDP(
            ip         = ip,
            ip_address = IP_ADDRESS,
        )

        # Get UDP port
        udp_port = udp.crossbar.get_port(UDP_PORT, 32)

        # Connect UDP source (RX)
        self.comb += [
            self.udp_source_valid.eq(udp_port.source.valid),
            self.udp_source_last.eq(udp_port.source.last),
            udp_port.source.ready.eq(self.udp_source_ready),
            self.udp_source_src_port.eq(udp_port.source.src_port),
            self.udp_source_dst_port.eq(udp_port.source.dst_port),
            self.udp_source_ip_address.eq(udp_port.source.ip_address),
            self.udp_source_length.eq(udp_port.source.length),
            self.udp_source_data.eq(udp_port.source.data),
            self.udp_source_error.eq(udp_port.source.error),
        ]

        # Connect UDP sink (TX)
        self.comb += [
            udp_port.sink.valid.eq(self.udp_sink_valid),
            udp_port.sink.last.eq(self.udp_sink_last),
            self.udp_sink_ready.eq(udp_port.sink.ready),
            udp_port.sink.src_port.eq(self.udp_sink_src_port),
            udp_port.sink.dst_port.eq(self.udp_sink_dst_port),
            udp_port.sink.ip_address.eq(self.udp_sink_ip_address),
            udp_port.sink.length.eq(self.udp_sink_length),
            udp_port.sink.data.eq(self.udp_sink_data),
            udp_port.sink.error.eq(self.udp_sink_error),
        ]


def main():
    print(f"Generating LiteEth UDP core for 5A-75E:")
    print(f"  MAC: {MAC_ADDRESS:012x}")
    print(f"  IP:  {(IP_ADDRESS>>24)&0xff}.{(IP_ADDRESS>>16)&0xff}.{(IP_ADDRESS>>8)&0xff}.{IP_ADDRESS&0xff}")
    print(f"  Port: {UDP_PORT}")
    print(f"  RX Delay: {RX_DELAY*1e9:.1f}ns")
    print(f"  TX Delay: {TX_DELAY*1e9:.1f}ns")

    core = LiteEthCore()

    # Define all IOs
    ios = {
        core.sys_clock,
        core.sys_reset,
        core.rgmii_eth_clocks_tx,
        core.rgmii_eth_clocks_rx,
        core.rgmii_eth_rst_n,
        core.rgmii_eth_int_n,
        core.rgmii_eth_mdio,
        core.rgmii_eth_mdc,
        core.rgmii_eth_rx_ctl,
        core.rgmii_eth_rx_data,
        core.rgmii_eth_tx_ctl,
        core.rgmii_eth_tx_data,
        core.udp_source_valid,
        core.udp_source_last,
        core.udp_source_ready,
        core.udp_source_src_port,
        core.udp_source_dst_port,
        core.udp_source_ip_address,
        core.udp_source_length,
        core.udp_source_data,
        core.udp_source_error,
        core.udp_sink_valid,
        core.udp_sink_last,
        core.udp_sink_ready,
        core.udp_sink_src_port,
        core.udp_sink_dst_port,
        core.udp_sink_ip_address,
        core.udp_sink_length,
        core.udp_sink_data,
        core.udp_sink_error,
    }

    # Generate Verilog
    v = verilog.convert(core, ios=ios, name="liteeth_core")

    output_file = "liteeth_core_new.v"
    with open(output_file, "w") as f:
        f.write(str(v))

    print(f"\nGenerated: {output_file}")


if __name__ == "__main__":
    main()
