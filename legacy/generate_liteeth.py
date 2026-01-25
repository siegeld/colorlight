#!/usr/bin/env python3
"""
Generate LiteEth UDP core for Colorlight 5A-75E with RTL8211FD PHY.

The RTL8211FD requires specific RGMII timing - it can add internal TX/RX delays
via strap pins or MDIO configuration. The ECP5 RGMII PHY in LiteEth handles
the FPGA-side delays.
"""

import argparse
import os

from migen import *

from litex.gen import *
from litex.soc.interconnect import stream
from litex.soc.interconnect.csr import AutoCSR

from liteeth.common import *
from liteeth.phy.ecp5rgmii import LiteEthPHYRGMII
from liteeth.core import LiteEthUDPIPCore


class LiteEthUDPCore(Module, AutoCSR):
    def __init__(self, platform, phy, mac_address, ip_address, udp_port,
                 clk_freq=125e6, with_icmp=True, tx_fifo_depth=2048, rx_fifo_depth=2048):

        # Create UDP/IP core
        self.submodules.core = LiteEthUDPIPCore(
            phy         = phy,
            mac_address = mac_address,
            ip_address  = ip_address,
            clk_freq    = clk_freq,
            with_icmp   = with_icmp,
        )

        # UDP port
        self.udp_port = udp_port

        # Create UDP sink (for sending - directly expose)
        self.submodules.udp_sink = udp_sink = stream.Endpoint(eth_udp_user_description(32))

        # Create UDP source (for receiving - connect to crossbar)
        self.submodules.udp_source = udp_source = stream.Endpoint(eth_udp_user_description(32))

        # Add UDP port to crossbar
        udp_crossbar_port = self.core.udp.crossbar.get_port(udp_port, 32)
        self.comb += [
            # RX: crossbar -> source
            udp_crossbar_port.source.connect(udp_source),
            # TX: sink -> crossbar
            udp_sink.connect(udp_crossbar_port.sink),
        ]


def generate_core(
    output_dir=".",
    mac_address=0x10E2D5000001,
    ip_address="10.11.6.250",
    udp_port=6000,
    clk_freq=125e6,
    rx_delay=2e-9,
    tx_delay=2e-9,
):
    """Generate the LiteEth core Verilog file."""

    from migen.fhdl.verilog import convert

    # Parse IP address
    ip_parts = [int(x) for x in ip_address.split(".")]
    ip_int = (ip_parts[0] << 24) | (ip_parts[1] << 16) | (ip_parts[2] << 8) | ip_parts[3]

    print(f"Generating LiteEth UDP core:")
    print(f"  MAC Address: {mac_address:012x}")
    print(f"  IP Address:  {ip_address} ({ip_int})")
    print(f"  UDP Port:    {udp_port}")
    print(f"  Clock Freq:  {clk_freq/1e6} MHz")
    print(f"  RX Delay:    {rx_delay*1e9} ns")
    print(f"  TX Delay:    {tx_delay*1e9} ns")

    # Create a minimal platform for signal definitions
    class Platform:
        device = "LFE5U-25F"
        def request(self, name, number=None, loose=False):
            if name == "eth_clocks":
                return Record([("tx", 1), ("rx", 1)])
            elif name == "eth":
                return Record([
                    ("rst_n", 1),
                    ("int_n", 1),
                    ("mdio", 1),
                    ("mdc", 1),
                    ("rx_ctl", 1),
                    ("rx_data", 4),
                    ("tx_ctl", 1),
                    ("tx_data", 4),
                ])
            return None

    platform = Platform()

    # Create module
    class Top(Module):
        def __init__(self):
            # Clock and reset
            self.clock_domains.cd_sys = ClockDomain()
            self.clock_domains.cd_eth_rx = ClockDomain()
            self.clock_domains.cd_eth_tx = ClockDomain()

            # RGMII interface signals
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

            # Create RGMII PHY
            # For RTL8211FD, we need to handle delays
            # The ECP5 PHY uses DELAYF primitives for RX delay
            self.submodules.ethphy = ethphy = LiteEthPHYRGMII(
                clock_pads = Record([("tx", self.rgmii_eth_clocks_tx), ("rx", self.rgmii_eth_clocks_rx)]),
                pads       = Record([
                    ("rst_n", self.rgmii_eth_rst_n),
                    ("int_n", self.rgmii_eth_int_n),
                    ("mdio", self.rgmii_eth_mdio),
                    ("mdc", self.rgmii_eth_mdc),
                    ("rx_ctl", self.rgmii_eth_rx_ctl),
                    ("rx_data", self.rgmii_eth_rx_data),
                    ("tx_ctl", self.rgmii_eth_tx_ctl),
                    ("tx_data", self.rgmii_eth_tx_data),
                ]),
                tx_delay   = tx_delay,
                rx_delay   = rx_delay,
            )

            # Connect clock domains
            self.comb += [
                self.cd_eth_rx.clk.eq(ethphy.crg.cd_eth_rx.clk),
                self.cd_eth_tx.clk.eq(ethphy.crg.cd_eth_tx.clk),
            ]

            # Create UDP/IP core
            self.submodules.core = LiteEthUDPIPCore(
                phy         = ethphy,
                mac_address = mac_address,
                ip_address  = ip_int,
                clk_freq    = int(clk_freq),
                with_icmp   = True,
            )

            # UDP port interface
            udp_port_if = self.core.udp.crossbar.get_port(udp_port, 32)

            # Expose UDP source (received packets)
            self.udp_source_valid = Signal()
            self.udp_source_last = Signal()
            self.udp_source_ready = Signal()
            self.udp_source_src_port = Signal(16)
            self.udp_source_dst_port = Signal(16)
            self.udp_source_ip_address = Signal(32)
            self.udp_source_length = Signal(16)
            self.udp_source_data = Signal(32)
            self.udp_source_error = Signal(4)

            self.comb += [
                self.udp_source_valid.eq(udp_port_if.source.valid),
                self.udp_source_last.eq(udp_port_if.source.last),
                udp_port_if.source.ready.eq(self.udp_source_ready),
                self.udp_source_src_port.eq(udp_port_if.source.src_port),
                self.udp_source_dst_port.eq(udp_port_if.source.dst_port),
                self.udp_source_ip_address.eq(udp_port_if.source.ip_address),
                self.udp_source_length.eq(udp_port_if.source.length),
                self.udp_source_data.eq(udp_port_if.source.data),
                self.udp_source_error.eq(udp_port_if.source.error),
            ]

            # Expose UDP sink (transmit packets)
            self.udp_sink_valid = Signal()
            self.udp_sink_last = Signal()
            self.udp_sink_ready = Signal()
            self.udp_sink_src_port = Signal(16)
            self.udp_sink_dst_port = Signal(16)
            self.udp_sink_ip_address = Signal(32)
            self.udp_sink_length = Signal(16)
            self.udp_sink_data = Signal(32)
            self.udp_sink_error = Signal(4)

            self.comb += [
                udp_port_if.sink.valid.eq(self.udp_sink_valid),
                udp_port_if.sink.last.eq(self.udp_sink_last),
                self.udp_sink_ready.eq(udp_port_if.sink.ready),
                udp_port_if.sink.src_port.eq(self.udp_sink_src_port),
                udp_port_if.sink.dst_port.eq(self.udp_sink_dst_port),
                udp_port_if.sink.ip_address.eq(self.udp_sink_ip_address),
                udp_port_if.sink.length.eq(self.udp_sink_length),
                udp_port_if.sink.data.eq(self.udp_sink_data),
                udp_port_if.sink.error.eq(self.udp_sink_error),
            ]

    # Generate
    top = Top()

    # Convert to Verilog
    ios = {
        top.cd_sys.clk,
        top.cd_sys.rst,
        top.rgmii_eth_clocks_tx,
        top.rgmii_eth_clocks_rx,
        top.rgmii_eth_rst_n,
        top.rgmii_eth_int_n,
        top.rgmii_eth_mdio,
        top.rgmii_eth_mdc,
        top.rgmii_eth_rx_ctl,
        top.rgmii_eth_rx_data,
        top.rgmii_eth_tx_ctl,
        top.rgmii_eth_tx_data,
        top.udp_source_valid,
        top.udp_source_last,
        top.udp_source_ready,
        top.udp_source_src_port,
        top.udp_source_dst_port,
        top.udp_source_ip_address,
        top.udp_source_length,
        top.udp_source_data,
        top.udp_source_error,
        top.udp_sink_valid,
        top.udp_sink_last,
        top.udp_sink_ready,
        top.udp_sink_src_port,
        top.udp_sink_dst_port,
        top.udp_sink_ip_address,
        top.udp_sink_length,
        top.udp_sink_data,
        top.udp_sink_error,
    }

    output_file = os.path.join(output_dir, "liteeth_core.v")

    verilog = convert(top, ios=ios, name="liteeth_core")

    with open(output_file, "w") as f:
        f.write(str(verilog))

    print(f"\nGenerated: {output_file}")
    return output_file


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Generate LiteEth UDP core for 5A-75E")
    parser.add_argument("--output-dir", default=".", help="Output directory")
    parser.add_argument("--mac", default="0x10E2D5000001", help="MAC address (hex)")
    parser.add_argument("--ip", default="10.11.6.250", help="IP address")
    parser.add_argument("--port", type=int, default=6000, help="UDP port")
    parser.add_argument("--rx-delay", type=float, default=2e-9, help="RX delay in seconds")
    parser.add_argument("--tx-delay", type=float, default=2e-9, help="TX delay in seconds")

    args = parser.parse_args()

    generate_core(
        output_dir=args.output_dir,
        mac_address=int(args.mac, 16),
        ip_address=args.ip,
        udp_port=args.port,
        rx_delay=args.rx_delay,
        tx_delay=args.tx_delay,
    )
