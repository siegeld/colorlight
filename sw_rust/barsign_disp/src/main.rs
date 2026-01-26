#![no_std]
#![no_main]

use core::convert::TryInto;
use core::fmt::Write as _;

use barsign_disp::*;
use bitmap_udp::BitmapReceiver;
use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::blocking::serial::Write;
use embedded_hal::serial::Read;
use ethernet::Eth;
use hal::*;
use heapless::Vec;
use layout::LayoutConfig;
use litex_pac as pac;
use riscv_rt::entry;
use tftp_config::TftpConfigLoader;
use smoltcp::iface::{InterfaceBuilder, NeighborCache, Routes};
use smoltcp::socket::{
    Dhcpv4Event, Dhcpv4Socket, TcpSocket, TcpSocketBuffer, UdpPacketMetadata, UdpSocket,
    UdpSocketBuffer,
};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

#[entry]
fn main() -> ! {
    let peripherals = unsafe { pac::Peripherals::steal() };

    let mut serial = UART {
        registers: peripherals.uart,
    };

    serial.bwrite_all(b"Hello world!\n").unwrap();

    let mut hub75 = hub75::Hub75::new(peripherals.hub75, peripherals.hub75_palette);

    // Read flash unique ID before Flash takes ownership of SPI peripheral
    let unique_id = flash_id::read_flash_unique_id(&peripherals.spiflash_mmap);
    let mac_bytes = flash_id::derive_mac(&unique_id);

    let mut flash = img_flash::Flash::new(peripherals.spiflash_mmap);
    let mut delay = TIMER {
        registers: peripherals.timer0,
        sys_clk: 40_000_000,  // 40MHz system clock
    };

    // Print startup info
    writeln!(serial, "Flash UID: {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        unique_id[0], unique_id[1], unique_id[2], unique_id[3],
        unique_id[4], unique_id[5], unique_id[6], unique_id[7]).ok();
    writeln!(serial, "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac_bytes[0], mac_bytes[1], mac_bytes[2],
        mac_bytes[3], mac_bytes[4], mac_bytes[5]).ok();

    let mut buffer = [0u8; 64];
    let out_data = heapless::Vec::new();
    let mut output = menu::Output { serial, out_data };

    // Start with no IP — DHCP will configure it
    let device = Eth::new(peripherals.ethmac, peripherals.ethmem);
    let mut neighbor_cache_entries = [None; 8];
    let neighbor_cache = NeighborCache::new(&mut neighbor_cache_entries[..]);
    let mut ip_addrs = [IpCidr::new(IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), 0)];
    let mut routes_storage = [None; 1];
    let routes = Routes::new(&mut routes_storage[..]);
    let mut sockets_entries: [_; 5] = Default::default();
    let mut iface = InterfaceBuilder::new(device, &mut sockets_entries[..])
        .hardware_addr(EthernetAddress::from_bytes(&mac_bytes).into())
        .neighbor_cache(neighbor_cache)
        .ip_addrs(&mut ip_addrs[..])
        .routes(routes)
        .finalize();

    let tcp_server_socket = {
        static mut TCP_SERVER_RX_DATA: [u8; 256] = [0; 256];
        static mut TCP_SERVER_TX_DATA: [u8; 256] = [0; 256];
        let tcp_rx_buffer = TcpSocketBuffer::new(unsafe { &mut TCP_SERVER_RX_DATA[..] });
        let tcp_tx_buffer = TcpSocketBuffer::new(unsafe { &mut TCP_SERVER_TX_DATA[..] });
        TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer)
    };

    let udp_server_socket = {
        static mut UDP_SERVER_RX_DATA: [u8; 2048] = [0; 2048];
        static mut UDP_SERVER_TX_DATA: [u8; 2048] = [0; 2048];
        static mut UDP_SERVER_RX_METADATA: [UdpPacketMetadata; 32] = [UdpPacketMetadata::EMPTY; 32];
        static mut UDP_SERVER_TX_METADATA: [UdpPacketMetadata; 32] = [UdpPacketMetadata::EMPTY; 32];
        let udp_rx_buffer = unsafe {
            UdpSocketBuffer::new(&mut UDP_SERVER_RX_METADATA[..], &mut UDP_SERVER_RX_DATA[..])
        };
        let udp_tx_buffer = unsafe {
            UdpSocketBuffer::new(&mut UDP_SERVER_TX_METADATA[..], &mut UDP_SERVER_TX_DATA[..])
        };
        UdpSocket::new(udp_rx_buffer, udp_tx_buffer)
    };

    let bitmap_udp_socket = {
        static mut BITMAP_UDP_RX_DATA: [u8; 16384] = [0; 16384];
        static mut BITMAP_UDP_TX_DATA: [u8; 64] = [0; 64];
        static mut BITMAP_UDP_RX_META: [UdpPacketMetadata; 12] = [UdpPacketMetadata::EMPTY; 12];
        static mut BITMAP_UDP_TX_META: [UdpPacketMetadata; 1] = [UdpPacketMetadata::EMPTY; 1];
        let rx = unsafe {
            UdpSocketBuffer::new(&mut BITMAP_UDP_RX_META[..], &mut BITMAP_UDP_RX_DATA[..])
        };
        let tx = unsafe {
            UdpSocketBuffer::new(&mut BITMAP_UDP_TX_META[..], &mut BITMAP_UDP_TX_DATA[..])
        };
        UdpSocket::new(rx, tx)
    };

    let tftp_udp_socket = {
        static mut TFTP_UDP_RX_DATA: [u8; 1024] = [0; 1024];
        static mut TFTP_UDP_TX_DATA: [u8; 128] = [0; 128];
        static mut TFTP_UDP_RX_META: [UdpPacketMetadata; 4] = [UdpPacketMetadata::EMPTY; 4];
        static mut TFTP_UDP_TX_META: [UdpPacketMetadata; 4] = [UdpPacketMetadata::EMPTY; 4];
        let rx = unsafe {
            UdpSocketBuffer::new(&mut TFTP_UDP_RX_META[..], &mut TFTP_UDP_RX_DATA[..])
        };
        let tx = unsafe {
            UdpSocketBuffer::new(&mut TFTP_UDP_TX_META[..], &mut TFTP_UDP_TX_DATA[..])
        };
        UdpSocket::new(rx, tx)
    };

    let dhcp_socket = Dhcpv4Socket::new();

    let tcp_server_handle = iface.add_socket(tcp_server_socket);
    let udp_server_handle = iface.add_socket(udp_server_socket);
    let bitmap_udp_handle = iface.add_socket(bitmap_udp_socket);
    let tftp_udp_handle = iface.add_socket(tftp_udp_socket);
    let dhcp_handle = iface.add_socket(dhcp_socket);

    // Always turn on HUB75 for debugging (shows firmware is running)
    hub75.on();

    // Load image from SPI flash if available, otherwise use default
    if let Ok(image) = img::load_image(flash.read_image()) {
        hub75.set_img_param(image.0, image.1);
        hub75.write_img_data(0, image.3);
        hub75.swap_buffers();
    } else {
        let image = img::load_default_image();
        hub75.set_img_param(image.0, image.1);
        hub75.write_img_data(0, image.3);
        hub75.swap_buffers();
    }

    // Configure panel: single 128x64 panel, one chain position
    hub75.set_panel_param(0, 0, 0, 0, 0);  // x=0, y=0, no rotation

    // Debug: print panel params to verify they were set
    let (x0, y0, r0) = hub75.get_panel_param(0, 0);
    writeln!(output.serial, "Panel config set: p0_0=({},{},{})", x0, y0, r0).ok();

    let context = menu::Context {
        mac: mac_bytes,
        output,
        hub75,
        flash,
        animation: menu::Animation::None,
        quit: false,
        debug: false,
        bitmap_stats: bitmap_udp::BitmapStats::new(),
        layout: LayoutConfig::single_panel(96, 48),
    };

    let mut r = menu::Runner::new(&menu::ROOT_MENU, &mut buffer, context);

    let mut bitmap_rx = BitmapReceiver::new();
    let mut tftp_loader = TftpConfigLoader::new();

    let mut time_ms: i64 = 0;
    let mut telnet_active = false;
    let mut loop_counter: u32 = 0;
    // Telnet IAC parser state: 0=normal, 1=got IAC(0xFF), 2=got cmd(WILL/WONT/DO/DONT),
    // 3=in subnegotiation, 4=got IAC inside subnegotiation
    let mut iac_state: u8 = 0;

    loop {
        // Use real timing: poll every 1ms
        delay.delay_ms(1u32);
        time_ms += 1;
        let time = Instant::from_millis(time_ms);

        loop_counter = loop_counter.wrapping_add(1);

        iface.poll(time).ok();

        // DHCP: poll for configuration changes
        {
            let socket = iface.get_socket::<Dhcpv4Socket>(dhcp_handle);
            if let Some(event) = socket.poll() {
                match event {
                    Dhcpv4Event::Configured(config) => {
                        writeln!(r.context.output, "DHCP: {}", config.address).ok();
                        iface.update_ip_addrs(|addrs| {
                            addrs[0] = IpCidr::Ipv4(config.address);
                        });
                        if let Some(router) = config.router {
                            iface.routes_mut().add_default_ipv4_route(router).ok();
                            // Start TFTP config load from gateway
                            if !tftp_loader.is_active() && !tftp_loader.is_done() {
                                writeln!(r.context.output, "TFTP: fetching layout.cfg from {}", router).ok();
                                tftp_loader.start(router);
                            }
                        }
                    }
                    Dhcpv4Event::Deconfigured => {
                        writeln!(r.context.output, "DHCP: lost lease").ok();
                        iface.update_ip_addrs(|addrs| {
                            addrs[0] = IpCidr::new(IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), 0);
                        });
                        iface.routes_mut().remove_default_ipv4_route();
                    }
                }
            }
        }

        // Static IP fallback after 10 seconds if DHCP hasn't configured an address
        if time_ms == 10_000 {
            if iface.ip_addrs()[0].address() == IpAddress::Ipv4(Ipv4Address::UNSPECIFIED) {
                let fallback = Ipv4Cidr::new(Ipv4Address([10, 11, 6, 250]), 24);
                writeln!(r.context.output, "DHCP timeout, using {}", fallback).ok();
                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::Ipv4(fallback);
                });
            }
        }

        // Poll TFTP config loader
        if tftp_loader.is_active() {
            let socket = iface.get_socket::<UdpSocket>(tftp_udp_handle);
            if tftp_loader.poll(socket, time_ms) {
                // Config loaded — parse and apply layout
                if let Some(layout) = tftp_loader.parse_config() {
                    writeln!(r.context.output, "TFTP: layout {}x{} ({}x{} virtual)",
                        layout.grid_cols, layout.grid_rows,
                        layout.virtual_width(), layout.virtual_height()).ok();
                    layout.apply(&mut r.context.hub75);
                    r.context.layout = layout;
                } else {
                    writeln!(r.context.output, "TFTP: failed to parse layout.cfg").ok();
                }
            }
        }

        // Update animation at ~30fps (every 33ms)
        // Double buffering prevents tearing from SDRAM rewrite
        if time_ms % 33 == 0 {
            r.context.animation_tick();
        }

        // tcp:23: telnet for menu
        {
            let socket = iface.get_socket::<TcpSocket>(tcp_server_handle);
            if !socket.is_open() {
                socket.listen(23).ok();
            }
            if !telnet_active && socket.is_active() {
                iac_state = 0;
                r.context.output.out_data.clear();
                r.context
                    .output
                    .out_data
                    .extend_from_slice(
                        // Taken from https://stackoverflow.com/a/4532395
                        // Does magic telnet stuff to behave more like a dumb serial terminal
                        b"\xFF\xFD\x22\xFF\xFA\x22\x01\x00\xFF\xF0\xFF\xFB\x01\r\nWelcome to the menu. Use \"help\" for help\r\n> ",
                    )
                    .expect("Should always work");
            }
            telnet_active = socket.is_active();

            if socket.may_recv() {
                while socket.can_recv() {
                    let mut buffer = [0u8; 64];
                    let received = {
                        match socket.recv_slice(&mut buffer) {
                            Ok(received) => received,
                            _ => 0,
                        }
                    };

                    for byte in &buffer[..received] {
                        let b = *byte;
                        match iac_state {
                            0 => {
                                if b == 0xFF {
                                    iac_state = 1; // IAC
                                } else if b != 0 {
                                    r.input_byte(b);
                                }
                            }
                            1 => match b {
                                // WILL, WONT, DO, DONT: expect one option byte
                                0xFB | 0xFC | 0xFD | 0xFE => iac_state = 2,
                                // SB: subnegotiation start
                                0xFA => iac_state = 3,
                                // Anything else (including doubled 0xFF): done
                                _ => iac_state = 0,
                            },
                            2 => {
                                // Option byte consumed, back to normal
                                iac_state = 0;
                            }
                            3 => {
                                // In subnegotiation, wait for IAC
                                if b == 0xFF {
                                    iac_state = 4;
                                }
                            }
                            4 => {
                                // IAC inside subneg: SE(0xF0) ends it
                                if b == 0xF0 {
                                    iac_state = 0;
                                } else {
                                    iac_state = 3;
                                }
                            }
                            _ => iac_state = 0,
                        }
                    }

                    // socket.send_slice(core::slice::from_ref(&data)).unwrap();
                }
            } else if socket.can_send() {
                socket.close();
            }

            // Handle quit command
            if r.context.quit {
                r.context.quit = false;
                socket.close();
            }

            if socket.can_send() {
                if let Ok(sent) = socket.send_slice(&r.context.output.out_data) {
                    let new_data = Vec::from_slice(&r.context.output.out_data[sent..])
                        .expect("New size is the same as the old size, can never fail");
                    r.context.output.out_data = new_data;
                }
            }
        }
        // udp:6454: artnet
        {
            let socket = iface.get_socket::<UdpSocket>(udp_server_handle);
            if !socket.is_open() {
                socket.bind(6454).ok();
            }

            match socket.recv() {
                Ok((data, _endpoint)) => {
                    if let Ok((offset, data)) = artnet::packet2hub75(data) {
                        // Palette is set via the two *last* universes
                        let palette_offset = ((1 << 16) - 2) * 170;
                        if offset < palette_offset {
                            // r.context.hub75.write_img_data(offset, data);
                        } else {
                            r.context
                                .hub75
                                .set_palette((offset - palette_offset) as u8, data);
                        }
                        // writeln!(r.context.serial, "{}", offset);
                    }
                }
                Err(_) => (),
            };
        }
        // udp:7000: bitmap — drain all queued packets
        {
            let socket = iface.get_socket::<UdpSocket>(bitmap_udp_handle);
            if !socket.is_open() {
                socket.bind(7000).ok();
            }
            while socket.can_recv() {
                match socket.recv() {
                    Ok((data, _endpoint)) => {
                        let complete = bitmap_rx.process_packet(data, &mut r.context.hub75);
                        if r.context.debug {
                            let s = &bitmap_rx.stats;
                            writeln!(r.context.output, "BM: pkt={} chunk={}/{} {}x{} len={} mask={:04x}{}",
                                s.packets_total, s.last_chunk_index + 1, s.last_total_chunks,
                                s.last_width, s.last_height, s.last_data_len,
                                s.chunks_received,
                                if complete { " COMPLETE" } else { "" },
                            ).ok();
                        }
                        if complete {
                            r.context.hub75.swap_buffers();
                            r.context.hub75.set_mode(hub75::OutputMode::FullColor);
                            r.context.hub75.on();
                            r.context.animation = menu::Animation::None;
                        }
                        r.context.bitmap_stats = bitmap_rx.stats;
                    }
                    Err(_) => break,
                }
            }
        }
        if let Ok(data) = r.context.output.serial.read() {
            r.input_byte(if data == b'\n' { b'\r' } else { data });
        }
    }
}
