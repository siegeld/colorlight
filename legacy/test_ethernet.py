#!/usr/bin/env python3
"""
Simple Ethernet test for Colorlight 5A-75E V6.0/V8.x with RTL8211FD PHY.
Uses LiteX to build a minimal SoC with Ethernet and ping support.
"""

import os
import argparse

from migen import *

from litex.gen import *
from litex.build.io import DDROutput

from litex_boards.platforms import colorlight_5a_75e

from litex.soc.cores.clock import ECP5PLL
from litex.soc.integration.soc_core import *
from litex.soc.integration.builder import *

from liteeth.phy.ecp5rgmii import LiteEthPHYRGMII

# CRG (Clock Reset Generator) ---------------------------------------------------------------------

class _CRG(LiteXModule):
    def __init__(self, platform, sys_clk_freq):
        self.rst    = Signal(name="rst")
        self.cd_sys = ClockDomain(name="sys")

        # Clk / Rst
        clk25 = platform.request("clk25")

        # PLL
        self.pll = pll = ECP5PLL()
        self.comb += pll.reset.eq(self.rst)
        pll.register_clkin(clk25, 25e6)
        pll.create_clkout(self.cd_sys, sys_clk_freq)


# BaseSoC -----------------------------------------------------------------------------------------

class BaseSoC(SoCMini):
    def __init__(self, revision="6.0", sys_clk_freq=50e6,
                 ip_address="10.11.6.250", mac_address=0x10E2D5000001,
                 **kwargs):

        platform = colorlight_5a_75e.Platform(revision=revision)

        # CRG
        self.crg = _CRG(platform, sys_clk_freq)

        # SoCMini
        SoCMini.__init__(self, platform, sys_clk_freq, ident="LiteX Ethernet Test on 5A-75E")

        # Ethernet PHY
        self.ethphy = LiteEthPHYRGMII(
            clock_pads = platform.request("eth_clocks", 0),
            pads       = platform.request("eth", 0),
            tx_delay   = 0e-9,  # RTL8211FD handles TX delay internally
            rx_delay   = 2e-9,  # FPGA adds RX delay
        )

        # Parse IP address
        ip_parts = [int(x) for x in ip_address.split(".")]
        ip_int = (ip_parts[0] << 24) | (ip_parts[1] << 16) | (ip_parts[2] << 8) | ip_parts[3]

        # Add Ethernet with ICMP (ping) support
        from liteeth.mac import LiteEthMAC
        from liteeth.core import LiteEthUDPIPCore

        self.add_etherbone(
            phy         = self.ethphy,
            ip_address  = ip_address,
            mac_address = mac_address,
            udp_port    = 1234,
        )


def main():
    parser = argparse.ArgumentParser(description="LiteX Ethernet Test for 5A-75E")
    parser.add_argument("--build",      action="store_true", help="Build bitstream")
    parser.add_argument("--load",       action="store_true", help="Load bitstream")
    parser.add_argument("--revision",   default="6.0",       help="Board revision (6.0 or 7.1)")
    parser.add_argument("--ip",         default="10.11.6.250", help="IP address")
    parser.add_argument("--sys-clk-freq", default=50e6, type=float, help="System clock frequency")

    builder_args(parser)
    args = parser.parse_args()

    soc = BaseSoC(
        revision     = args.revision,
        sys_clk_freq = int(args.sys_clk_freq),
        ip_address   = args.ip,
    )

    builder = Builder(soc, **builder_argdict(args))
    if args.build:
        builder.build()

    if args.load:
        prog = soc.platform.create_programmer()
        prog.load_bitstream(builder.get_bitstream_filename(mode="sram"))


if __name__ == "__main__":
    main()
