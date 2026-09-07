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

from migen import If, Signal, Module, FSM, NextValue, NextState, Cat
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


class Hub75UdpDma(Module, AutoCSR):
    """Write streamed pixels straight into the framebuffer, bypassing the CPU.

    Consumes the UDP payload of the bitmap protocol (8-bit stream) and drives a
    LiteDRAMDMAWriter. This is the point of Tier 2: the CPU's per-pixel loop
    costs 3 loads + 4 stores per 4 pixels over an uncached bus with no D-cache,
    which measures out at a hard ~600 kpx/s. Hardware doing the same work is
    limited only by the line rate.

    Wire format, little-endian, 10-byte header:
        0..1  magic 'B','M'
        2..3  frame_id   u16
        4     chunk_index u8
        5     total_chunks u8
        6..7  width      u16
        8..9  height     u16
        then RGB888 pixels, chunk N carrying pixels N*pixels_per_chunk..

    The CPU still receives every packet through its own copy of the stream, so
    it keeps the arrival mask, completion detection, repair and buffer swap
    exactly as before. Only the pixel copy moves to hardware.
    """

    def __init__(self, sdram, udp_sink, pixels_per_chunk=487, fifo_depth=64,
                 stall_limit=8192):
        port = sdram.crossbar.get_port(mode="write", data_width=32)
        self.submodules.writer = writer = LiteDRAMDMAWriter(port, fifo_depth=fifo_depth)

        self.base = CSRStorage(24, description="Framebuffer word address the CPU is writing into")
        self.limit = CSRStorage(24, description="Pixels in the image; writes past this are dropped")
        self.ctrl = CSRStorage(fields=[
            CSRField("enable", description="Enable hardware pixel writes"),
        ])
        self.pixels = CSRStatus(32, description="Pixels written")
        self.chunks = CSRStatus(32, description="Chunks accepted")
        self.bad_magic = CSRStatus(32, description="Payloads rejected on magic")
        self.last_chunk = CSRStatus(8, description="Most recent chunk_index")
        self.status = CSRStatus(fields=[
            CSRField("busy", description="A payload is being written"),
        ])
        self.stalls = CSRStatus(32, description="Payloads abandoned after a DRAM stall")

        sink = udp_sink
        hdr_idx = Signal(4)
        chunk_index = Signal(8)
        pix_adr = Signal(24)
        sub = Signal(2)          # which byte of the RGB triple
        r = Signal(8)
        g = Signal(8)
        enable = self.ctrl.fields.enable
        # Watchdog. If the DRAM writer stops accepting -- the display DMA can
        # hold the bus for a long time -- this FSM would sit in PIX forever, and
        # because everything upstream backpressures into it the entire hardware
        # path wedges behind one packet. Measured: chunks froze at exactly 204
        # (3 frames) while the UDP filter kept counting. Abandon the payload
        # instead; a dropped chunk is repaired from the previous frame, a wedged
        # pipeline is not recoverable.
        stall = Signal(max=stall_limit + 1)
        progress = Signal()

        # HUB75 framebuffer word format is 0x00GGRRBB.
        word = Signal(32)
        self.comb += word.eq(Cat(sink.data, r, g, Signal(8)))

        self.comb += [
            writer.sink.address.eq(pix_adr),
            writer.sink.data.eq(word),
        ]

        self.submodules.fsm = fsm = FSM(reset_state="IDLE")
        fsm.act("IDLE",
            sink.ready.eq(1),
            If(sink.valid & enable,
                NextValue(hdr_idx, 1),
                If(sink.data == 0x42,          # 'B'
                    NextState("HDR"),
                ).Else(
                    NextValue(self.bad_magic.status, self.bad_magic.status + 1),
                    If(~sink.last, NextState("DROP")),
                )),
            # Consume and ignore while disabled.
            If(sink.valid & ~enable, sink.ready.eq(1)),
        )
        fsm.act("HDR",
            self.status.fields.busy.eq(1),
            sink.ready.eq(1),
            If(sink.valid,
                NextValue(hdr_idx, hdr_idx + 1),
                If((hdr_idx == 1) & (sink.data != 0x4D),   # 'M'
                    NextValue(self.bad_magic.status, self.bad_magic.status + 1),
                    NextState("DROP"),
                ),
                If(hdr_idx == 4,
                    NextValue(chunk_index, sink.data),
                    NextValue(self.last_chunk.status, sink.data),
                ),
                If(hdr_idx == 9,
                    # Header consumed; pixels start on the next beat.
                    NextValue(pix_adr, self.base.storage + chunk_index * pixels_per_chunk),
                    NextValue(sub, 0),
                    NextValue(self.chunks.status, self.chunks.status + 1),
                    NextState("PIX"),
                ),
                If(sink.last, NextState("IDLE")),
            ))
        self.sync += [
            If(~self.fsm.ongoing("PIX"),
                stall.eq(0),
            ).Elif(progress,
                stall.eq(0),
            ).Elif(stall != stall_limit,
                stall.eq(stall + 1),
            ),
        ]

        fsm.act("PIX",
            self.status.fields.busy.eq(1),
            If(stall == stall_limit,
                NextValue(self.stalls.status, self.stalls.status + 1),
                NextState("DROP"),
            ),
            # Only the third byte of each triple emits a word, so the first two
            # are always accepted; the third waits on the DMA.
            If(sub != 2,
                sink.ready.eq(1),
                If(sink.valid,
                    progress.eq(1),
                    If(sub == 0, NextValue(r, sink.data)).Else(NextValue(g, sink.data)),
                    NextValue(sub, sub + 1),
                ),
            ).Else(
                sink.ready.eq(writer.sink.ready),
                writer.sink.valid.eq(sink.valid & (pix_adr < (self.base.storage + self.limit.storage))),
                If(sink.valid & writer.sink.ready,
                    progress.eq(1),
                    NextValue(sub, 0),
                    NextValue(pix_adr, pix_adr + 1),
                    NextValue(self.pixels.status, self.pixels.status + 1),
                ),
            ),
            If(sink.valid & sink.ready & sink.last, NextState("IDLE")),
        )
        fsm.act("DROP",
            sink.ready.eq(1),
            If(sink.valid & sink.last, NextState("IDLE")),
        )
