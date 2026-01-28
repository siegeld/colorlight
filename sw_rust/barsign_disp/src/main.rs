#![no_std]
#![no_main]

use core::convert::TryInto;
use core::fmt::Write as _;

use barsign_disp::*;
use bitmap_udp::BitmapReceiver;
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

/// Check if a raw Ethernet frame is a UDP packet destined for the bitmap port (7000).
/// Layout: Ethernet(14) + IPv4(20, IHL=5) + UDP(8) = 42-byte header.
fn is_bitmap_udp(frame: &[u8]) -> bool {
    frame.len() >= 52 // 42 header + 10 bitmap header min
        && frame[12] == 0x08 && frame[13] == 0x00 // EtherType: IPv4
        && frame[14] & 0x0F == 5                   // IHL: 5 (no options)
        && frame[23] == 17                          // Protocol: UDP
        && frame[36] == 0x1B && frame[37] == 0x58   // UDP dst port: 7000
}

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
    let mut sockets_entries: [_; 7] = Default::default();
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
        static mut BITMAP_UDP_RX_DATA: [u8; 65536] = [0; 65536];
        static mut BITMAP_UDP_TX_DATA: [u8; 64] = [0; 64];
        static mut BITMAP_UDP_RX_META: [UdpPacketMetadata; 48] = [UdpPacketMetadata::EMPTY; 48];
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

    let http_tcp_socket_a = {
        static mut HTTP_TCP_RX_A: [u8; 512] = [0; 512];
        static mut HTTP_TCP_TX_A: [u8; 2048] = [0; 2048];
        let rx = TcpSocketBuffer::new(unsafe { &mut HTTP_TCP_RX_A[..] });
        let tx = TcpSocketBuffer::new(unsafe { &mut HTTP_TCP_TX_A[..] });
        TcpSocket::new(rx, tx)
    };
    let http_tcp_socket_b = {
        static mut HTTP_TCP_RX_B: [u8; 512] = [0; 512];
        static mut HTTP_TCP_TX_B: [u8; 2048] = [0; 2048];
        let rx = TcpSocketBuffer::new(unsafe { &mut HTTP_TCP_RX_B[..] });
        let tx = TcpSocketBuffer::new(unsafe { &mut HTTP_TCP_TX_B[..] });
        TcpSocket::new(rx, tx)
    };

    let dhcp_socket = Dhcpv4Socket::new();

    let tcp_server_handle = iface.add_socket(tcp_server_socket);
    let udp_server_handle = iface.add_socket(udp_server_socket);
    let bitmap_udp_handle = iface.add_socket(bitmap_udp_socket);
    let tftp_udp_handle = iface.add_socket(tftp_udp_socket);
    let dhcp_handle = iface.add_socket(dhcp_socket);
    let http_handle_a = iface.add_socket(http_tcp_socket_a);
    let http_handle_b = iface.add_socket(http_tcp_socket_b);

    // Bind bitmap UDP socket so poll() routes leaked packets to its buffer
    {
        let socket = iface.get_socket::<UdpSocket>(bitmap_udp_handle);
        socket.bind(7000).ok();
    }

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
        reboot_pending: false,
        boot_server: None,
        mac_overflow: 0,
        mac_preamble_err: 0,
        mac_crc_err: 0,
    };

    let mut r = menu::Runner::new(&menu::ROOT_MENU, &mut buffer, context);

    let mut bitmap_rx = BitmapReceiver::new();
    let mut tftp_loader = TftpConfigLoader::new();

    let mut time_ms: i64 = 0;
    let mut last_bitmap_packet_ms: i64 = 0;
    let mut telnet_active = false;
    let mut loop_counter: u32 = 0;
    // Telnet IAC parser state: 0=normal, 1=got IAC(0xFF), 2=got cmd(WILL/WONT/DO/DONT),
    // 3=in subnegotiation, 4=got IAC inside subnegotiation
    let mut iac_state: u8 = 0;

    // HTTP server state — two sockets so one can accept while the other closes
    let http_handles = [http_handle_a, http_handle_b];
    let mut http_requests = [http::HttpRequest::new(), http::HttpRequest::new()];
    let mut http_responses = [http::HttpResponse::new(), http::HttpResponse::new()];
    let mut http_response_sent = [0usize; 2];
    let mut http_close_at = [0i64; 2];
    let mut http_connected_at = [0i64; 2];

    // Configure timer0 for periodic 1ms ticks (non-blocking)
    unsafe {
        let t = &*pac::Timer0::ptr();
        t.en().write(|w| w.bits(0));
        t.reload().write(|w| w.bits(40_000 - 1));  // 40MHz / 1000 = 40000 cycles per ms
        t.load().write(|w| w.bits(40_000 - 1));
        t.en().write(|w| w.bits(1));
        t.ev_pending().write(|w| w.bits(1));        // clear any pending event
    }

    loop {
        // Non-blocking 1ms tick: check if timer fired
        let timer_fired = unsafe {
            let t = &*pac::Timer0::ptr();
            if t.ev_pending().read().bits() != 0 {
                t.ev_pending().write(|w| w.bits(1));  // clear
                true
            } else {
                false
            }
        };
        if timer_fired {
            time_ms += 1;
        }
        let time = Instant::from_millis(time_ms);

        loop_counter = loop_counter.wrapping_add(1);

        // Raw fast path: drain bitmap UDP packets directly from MAC hardware,
        // bypassing smoltcp to eliminate double-copy overhead.
        macro_rules! drain_raw_bitmap {
            () => {{
                let device = iface.device();
                let mut _found = false;
                loop {
                    match device.peek_rx() {
                        Some(frame) if is_bitmap_udp(frame) => {
                            _found = true;
                            last_bitmap_packet_ms = time_ms;
                            let complete = bitmap_rx.process_packet(
                                &frame[42..], &mut r.context.hub75, time_ms);
                            if complete {
                                r.context.hub75.swap_buffers();
                                r.context.hub75.set_mode(hub75::OutputMode::FullColor);
                                r.context.hub75.on();
                                r.context.animation = menu::Animation::None;

                            }
                            r.context.bitmap_stats = bitmap_rx.stats;
                            device.ack_rx();
                        }
                        _ => break,
                    }
                }
                _found
            }};
        }

        // Fast path: drain bitmap packets from MAC. Only call poll() when
        // a non-bitmap packet blocks the queue — poll() is expensive on a
        // 40MHz RISC-V and causes FIFO overflows if called unconditionally.
        drain_raw_bitmap!();

        // Streaming flag: true while bitmap packets are arriving.
        // Computed here (after initial drain) so the burst loop can use it.
        let streaming = time_ms - last_bitmap_packet_ms < 200;

        for _burst in 0..50 {
            if iface.device().peek_rx().is_none() { break; }

            if streaming {
                // During streaming, discard non-bitmap packets instead of
                // calling poll().  poll() processes TCP/DHCP state machines
                // which stall the CPU for milliseconds and overflow the
                // 8-slot MAC FIFO.  The sender already has our ARP entry.
                iface.device().ack_rx();
                drain_raw_bitmap!();
            } else {
                iface.poll(time).ok();

                let mut got_any = drain_raw_bitmap!();

                // Socket fallback: drain bitmap packets that poll() consumed
                {
                    let socket = iface.get_socket::<UdpSocket>(bitmap_udp_handle);
                    while let Ok((data, _ep)) = socket.recv() {
                        got_any = true;
                        last_bitmap_packet_ms = time_ms;
                        let complete = bitmap_rx.process_packet(
                            data, &mut r.context.hub75, time_ms);
                        if complete {
                            r.context.hub75.swap_buffers();
                            r.context.hub75.set_mode(hub75::OutputMode::FullColor);
                            r.context.hub75.on();
                            r.context.animation = menu::Animation::None;
                        }
                        r.context.bitmap_stats = bitmap_rx.stats;
                    }
                }

                if !got_any { break; }
            }
        }
        if streaming {
            continue;
        }
        if !timer_fired || (time_ms % 5 != 0) {
            continue;
        }

        // Read MAC hardware error counters
        let (ovf, pre, crc) = iface.device().mac_errors();
        r.context.mac_overflow = ovf;
        r.context.mac_preamble_err = pre;
        r.context.mac_crc_err = crc;

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
                        }
                        // Start TFTP config load — Option 66 or hardcoded fallback
                        if !tftp_loader.is_active() && !tftp_loader.is_done() {
                            use crate::menu::BootServerSource;
                            let (server, source) = if let Some(ip) = config.tftp_server_name {
                                (ip, BootServerSource::Option66)
                            } else {
                                (Ipv4Address([10, 11, 6, 65]), BootServerSource::Fallback)
                            };
                            r.context.boot_server = Some((server.0, source));
                            // Build MAC-based filename: 02-78-7b-21-ae-53.yml
                            let m = &r.context.mac;
                            let mut fname = [0u8; 21]; // "xx-xx-xx-xx-xx-xx.yml"
                            const HEX: &[u8; 16] = b"0123456789abcdef";
                            for i in 0..6 {
                                fname[i * 3] = HEX[(m[i] >> 4) as usize];
                                fname[i * 3 + 1] = HEX[(m[i] & 0xf) as usize];
                                if i < 5 { fname[i * 3 + 2] = b'-'; }
                            }
                            fname[17..21].copy_from_slice(b".yml");
                            let fname_str = core::str::from_utf8(&fname).unwrap_or("config.yml");
                            writeln!(r.context.output, "TFTP: fetching {} from {}", fname_str, server).ok();
                            tftp_loader.start(server, fname_str);
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
                    let w = layout.virtual_width();
                    let h = layout.virtual_height();
                    writeln!(r.context.output, "TFTP: layout {}x{} ({}x{} virtual)",
                        layout.grid_cols, layout.grid_rows, w, h).ok();
                    layout.apply(&mut r.context.hub75);
                    r.context.layout = layout;
                    // Redraw at new virtual size
                    let total = (w as u32) * (h as u32);
                    r.context.hub75.set_img_param(w, total as u32);
                    let (rb_w, rb_len) = r.context.hub75.get_img_param();
                    writeln!(r.context.output, "TFTP: set_img_param({}, {}) -> readback({}, {})",
                        w, total, rb_w, rb_len).ok();
                    r.context.hub75.write_img_data(0, patterns::grid(w, h));
                    r.context.hub75.swap_buffers();
                } else {
                    writeln!(r.context.output, "TFTP: failed to parse layout.cfg").ok();
                }
            }
        }

        // Update animation at ~30fps (every 33ms), but skip during streaming
        if !streaming && time_ms % 33 == 0 {
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
                        b"\xFF\xFD\x22\xFF\xFA\x22\x01\x00\xFF\xF0\xFF\xFB\x01",
                    )
                    .expect("Should always work");
                write!(r.context.output, "\r\nColorlight v{}\r\n> ",
                    env!("CARGO_PKG_VERSION")).ok();
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
        // Drain MAC between slow-path blocks to prevent bitmap UDP overflow
        drain_raw_bitmap!();
        iface.poll(time).ok();

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
        // (bitmap UDP is handled in the fast path at top of loop)
        drain_raw_bitmap!();
        iface.poll(time).ok();

        // tcp:80: HTTP server (two sockets — one accepts while the other closes)
        {
            let http_ip = match iface.ip_addrs()[0].address() {
                IpAddress::Ipv4(v4) => v4.0,
                _ => [0u8; 4],
            };
            for i in 0..2 {
                let socket = iface.get_socket::<TcpSocket>(http_handles[i]);
                // Recycle socket after graceful close
                if http_close_at[i] > 0 {
                    if !socket.is_active() || time_ms >= http_close_at[i] {
                        if socket.is_open() {
                            socket.abort();
                        }
                        http_close_at[i] = 0;
                    }
                }
                if !socket.is_open() {
                    http_requests[i].reset();
                    http_responses[i].data.clear();
                    http_response_sent[i] = 0;
                    http_connected_at[i] = 0;
                    socket.listen(80).ok();
                }
                // Abort stuck HTTP connections: remote closed or idle too long
                if socket.is_active() {
                    if http_connected_at[i] == 0 {
                        http_connected_at[i] = time_ms;
                    }
                    if !http_requests[i].is_complete() {
                        if !socket.may_recv() || time_ms - http_connected_at[i] > 5000 {
                            socket.abort();
                            http_connected_at[i] = 0;
                            continue;
                        }
                    }
                } else {
                    http_connected_at[i] = 0;
                }
                if socket.can_recv() && !http_requests[i].is_complete() {
                    let mut buf = [0u8; 128];
                    if let Ok(n) = socket.recv_slice(&mut buf) {
                        if http_requests[i].feed(&buf[..n]) {
                            http::handle_request(
                                &http_requests[i], &mut http_responses[i],
                                &mut r.context, http_ip,
                            );
                            http_response_sent[i] = 0;
                        }
                    }
                }
                if socket.can_send() && http_response_sent[i] < http_responses[i].data.len() {
                    if let Ok(sent) = socket.send_slice(&http_responses[i].data[http_response_sent[i]..]) {
                        http_response_sent[i] += sent;
                    }
                    if http_response_sent[i] >= http_responses[i].data.len() && http_responses[i].data.len() > 0 {
                        socket.close();
                        http_close_at[i] = time_ms + 50;
                        if r.context.reboot_pending {
                            // Allow TCP FIN to transmit before resetting
                            for _ in 0..100_000 { unsafe { core::arch::asm!("nop") }; }
                            unsafe { (*litex_pac::Ctrl::ptr()).reset().write(|w| w.soc_rst().set_bit()) };
                        }
                    }
                }
            }
        }
        if let Ok(data) = r.context.output.serial.read() {
            r.input_byte(if data == b'\n' { b'\r' } else { data });
        }
    }
}
