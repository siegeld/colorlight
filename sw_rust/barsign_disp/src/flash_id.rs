use litex_pac as pac;

/// Read the 64-bit unique ID from W25Q32JV SPI flash.
///
/// Uses command 0x4B followed by 4 dummy bytes, then reads 8 data bytes.
/// Must be called before img_flash::Flash takes ownership of the SPI peripheral.
pub fn read_flash_unique_id(spi: &pac::SpiflashMmap) -> [u8; 8] {
    // Configure SPI master: 8-bit transfers, single width, mask=1
    unsafe {
        spi.master_phyconfig()
            .write(|w| w.len().bits(8).width().bits(1).mask().bits(1));
    }

    // Assert CS
    spi.master_cs().write(|w| w.master_cs().set_bit());

    // Send command 0x4B (Read Unique ID)
    spi_transfer_byte(spi, 0x4B);

    // Send 4 dummy bytes
    for _ in 0..4 {
        spi_transfer_byte(spi, 0x00);
    }

    // Read 8 bytes of unique ID
    let mut uid = [0u8; 8];
    for byte in uid.iter_mut() {
        *byte = spi_transfer_byte(spi, 0x00);
    }

    // Deassert CS
    spi.master_cs().write(|w| w.master_cs().clear_bit());

    uid
}

/// Transfer a single byte over SPI master (full duplex: send + receive).
fn spi_transfer_byte(spi: &pac::SpiflashMmap, tx: u8) -> u8 {
    while spi.master_status().read().tx_ready().bit_is_clear() {}
    unsafe { spi.master_rxtx().write(|w| w.bits(tx as u32)) };
    while spi.master_status().read().rx_ready().bit_is_clear() {}
    spi.master_rxtx().read().bits() as u8
}

/// Derive a locally-administered unicast MAC from the 64-bit flash unique ID.
///
/// Format: 02:xx:xx:xx:xx:xx — the 0x02 prefix marks it as locally administered
/// per IEEE 802. The 5 payload bytes are produced by XOR-folding the 8-byte UID.
pub fn derive_mac(unique_id: &[u8; 8]) -> [u8; 6] {
    let mut mac = [0x02u8, 0, 0, 0, 0, 0];
    for i in 0..5 {
        mac[i + 1] = unique_id[i] ^ unique_id[i + 3];
    }
    mac
}
