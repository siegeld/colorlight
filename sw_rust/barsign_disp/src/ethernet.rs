// From  https://github.com/DerFetzer/colorlight-litex/blob/48f1d38a3fcdf51d0bced21897e245570c38a175/rust/eth_demo/src/ethernet.rs,
// Apache 2.0/MIT by DerFetzer
use litex_pac::{Ethmac, Ethmem};

use smoltcp::phy::{self, DeviceCapabilities};
use smoltcp::time::Instant;
use smoltcp::{Error, Result};

// LiteEth buffer layout: nrxslots RX buffers followed by ntxslots TX buffers,
// each SLOT_SIZE bytes, starting at the Ethmem base address.
// NOTE: The LiteX SVD generator only describes 2 RX buffers regardless of nrxslots,
// so the PAC's tx_buffer offsets are wrong when nrxslots > 2.
// We compute addresses directly from the base.
const SLOT_SIZE: usize = 2048;
const NRXSLOTS: usize = 8;

pub struct Eth {
    ethmac: Ethmac,
    ethbuf: Ethmem,
}

impl Eth {
    pub fn new(ethmac: Ethmac, ethbuf: Ethmem) -> Self {
        ethmac
            .sram_writer_ev_pending()
            .write(unsafe { |w| w.bits(1) });
        ethmac
            .sram_reader_ev_pending()
            .write(unsafe { |w| w.bits(1) });
        ethmac.sram_reader_slot().write(unsafe { |w| w.bits(0) });

        Eth { ethmac, ethbuf }
    }

    /// Get the base address of the Ethmem region
    fn buf_base(&self) -> *mut u8 {
        self.ethbuf.rx_buffer_0(0) as *const _ as *mut u8
    }

    /// Read MAC hardware error counters: (overflow, preamble_errors, crc_errors)
    pub fn mac_errors(&self) -> (u32, u32, u32) {
        (
            self.ethmac.sram_writer_errors().read().bits(),
            self.ethmac.rx_datapath_preamble_errors().read().bits(),
            self.ethmac.rx_datapath_crc_errors().read().bits(),
        )
    }

    /// Peek at the current MAC RX slot without consuming it.
    /// Returns the raw Ethernet frame if a packet is pending.
    /// Caller must finish using the data before calling `ack_rx()`.
    pub fn peek_rx(&self) -> Option<&[u8]> {
        if self.ethmac.sram_writer_ev_pending().read().bits() == 0 {
            return None;
        }
        unsafe {
            let slot = self.ethmac.sram_writer_slot().read().bits() as usize;
            let length = self.ethmac.sram_writer_length().read().bits() as usize;
            let buf = self.buf_base() as *const u8;
            Some(core::slice::from_raw_parts(buf.add(slot * SLOT_SIZE), length))
        }
    }

    /// Acknowledge the current RX slot, allowing the MAC to reuse it.
    pub fn ack_rx(&self) {
        self.ethmac
            .sram_writer_ev_pending()
            .write(unsafe { |w| w.bits(1) });
    }
}

impl<'a> phy::Device<'a> for Eth {
    type RxToken = EthRxToken<'a>;
    type TxToken = EthTxToken<'a>;

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        if self.ethmac.sram_writer_ev_pending().read().bits() == 0 {
            return None;
        }
        let base = self.buf_base();
        Some((
            Self::RxToken {
                ethmac: &self.ethmac,
                base,
            },
            Self::TxToken {
                ethmac: &self.ethmac,
                base,
            },
        ))
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        let base = self.buf_base();
        Some(Self::TxToken {
            ethmac: &self.ethmac,
            base,
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 2048;
        caps.max_burst_size = Some(NRXSLOTS);
        caps
    }
}

pub struct EthRxToken<'a> {
    ethmac: &'a Ethmac,
    base: *mut u8,
}

impl<'a> phy::RxToken for EthRxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        unsafe {
            if self.ethmac.sram_writer_ev_pending().read().bits() == 0 {
                return Err(Error::Exhausted);
            }
            let slot = self.ethmac.sram_writer_slot().read().bits() as usize;
            let length = self.ethmac.sram_writer_length().read().bits() as usize;
            let buf = self.base.add(slot * SLOT_SIZE);
            let data = core::slice::from_raw_parts_mut(buf, length);
            let result = f(data);
            self.ethmac.sram_writer_ev_pending().write(|w| w.bits(1));
            result
        }
    }
}

pub struct EthTxToken<'a> {
    ethmac: &'a Ethmac,
    base: *mut u8,
}

impl<'a> phy::TxToken for EthTxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, len: usize, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        //#[link_section = ".main_ram"]
        static mut TX_BUFFER: [u8; 2048] = [0; 2048];
        static mut SLOT: u8 = 0;

        while self.ethmac.sram_reader_ready().read().bits() == 0 {}
        let result = f(unsafe { &mut TX_BUFFER[..len] });
        let current_slot = unsafe { SLOT } as usize;
        // TX buffers start after NRXSLOTS RX buffers
        unsafe {
            let tx_buf = self.base.add((NRXSLOTS + current_slot) * SLOT_SIZE);
            for i in 0..len {
                core::ptr::write_volatile(tx_buf.add(i), TX_BUFFER[i]);
            }
        }
        self.ethmac
            .sram_reader_slot()
            .write(unsafe { |w| w.bits(current_slot as u32) });
        self.ethmac
            .sram_reader_length()
            .write(unsafe { |w| w.bits(len as u32) });
        self.ethmac
            .sram_reader_start()
            .write(unsafe { |w| w.bits(1) });
        unsafe {
            SLOT = (SLOT + 1) % 2;
        }
        result
    }
}
