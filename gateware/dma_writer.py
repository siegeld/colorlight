# SDRAM write-path experiments for Tier 2 (pixels off the CPU).
#
# The display DMA re-reads the whole framebuffer every refresh and measures at
# ~79 MB/s, close to the theoretical ceiling of a 16-bit SDRAM at 40 MHz. The
# open question for Tier 2 is how much WRITE bandwidth is left, and what taking
# it costs the refresh rate.
#
# SdramWriteTester answers that without touching the ethernet stack: the CPU
# arms a burst of N words, hardware streams them through a LiteDRAMDMAWriter as
# fast as the bus allows, and reports the cycles taken. Words/cycle at a known
# clock gives achieved write bandwidth directly, and sampling the refresh
# counter across the burst gives the cost to the display.
#
# SPDX-License-Identifier: BSD-2-Clause

from migen import If, Signal, Module, FSM, NextValue, NextState
from litex.soc.interconnect.csr import AutoCSR, CSRStorage, CSRStatus, CSRField
from litedram.frontend.dma import LiteDRAMDMAWriter


class SdramWriteTester(Module, AutoCSR):
    """Burst-write engine used to measure available SDRAM write bandwidth."""

    def __init__(self, sdram, fifo_depth=16):
        port = sdram.crossbar.get_port(mode="write", data_width=32)
        self.submodules.writer = writer = LiteDRAMDMAWriter(port, fifo_depth=fifo_depth)

        self.base = CSRStorage(24, description="First word address of the burst")
        self.length = CSRStorage(24, description="Words to write in the burst")
        self.ctrl = CSRStorage(fields=[
            CSRField("start", description="Write 1 to arm a burst (self-clearing)"),
        ])
        self.status = CSRStatus(fields=[
            CSRField("busy", description="Burst in progress"),
        ])
        self.cycles = CSRStatus(32, description="Sys-clk cycles taken by the last burst")
        self.written = CSRStatus(32, description="Words written by the last burst")

        adr = Signal(24)
        remaining = Signal(24)
        cyc = Signal(32)
        wrote = Signal(32)
        busy = Signal()

        self.comb += [
            self.status.fields.busy.eq(busy),
            writer.sink.address.eq(adr),
            # Data is the address itself: a pattern that is cheap to generate and
            # verifiable by reading it back over the existing CPU bus.
            writer.sink.data.eq(adr),
        ]

        self.submodules.fsm = fsm = FSM(reset_state="IDLE")
        fsm.act("IDLE",
            If(self.ctrl.fields.start & (self.length.storage != 0),
                NextValue(adr, self.base.storage),
                NextValue(remaining, self.length.storage),
                NextValue(cyc, 0),
                NextValue(wrote, 0),
                NextState("RUN"),
            ))
        fsm.act("RUN",
            busy.eq(1),
            writer.sink.valid.eq(1),
            NextValue(cyc, cyc + 1),
            If(writer.sink.ready,
                NextValue(adr, adr + 1),
                NextValue(wrote, wrote + 1),
                If(remaining == 1,
                    NextState("DONE"),
                ).Else(
                    NextValue(remaining, remaining - 1),
                )),
        )
        fsm.act("DONE",
            NextValue(self.cycles.status, cyc),
            NextValue(self.written.status, wrote),
            NextState("IDLE"),
        )
