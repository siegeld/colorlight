//! Interrupt-driven network stack.
//!
//! All network state lives in statics so the ISR can access it.
//! The ISR calls network_handler() which does all socket processing.
//! Main loop does ZERO network code - only display/animation.

use core::mem::MaybeUninit;
use crate::ethernet::Eth;
use crate::bitmap_udp::{BitmapReceiver, BitmapStats};
use crate::hub75::Hub75;
use crate::tftp_config::TftpConfigLoader;
use crate::layout::LayoutConfig;
use crate::http::{HttpRequest, HttpResponse};

use smoltcp::iface::{Interface, InterfaceBuilder, NeighborCache, Routes, SocketHandle, SocketStorage};
use smoltcp::socket::{
    Dhcpv4Event, Dhcpv4Socket, TcpSocket, TcpSocketBuffer, UdpPacketMetadata, UdpSocket,
    UdpSocketBuffer,
};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

use litex_pac as pac;

// ============================================================================
// Static Storage
// ============================================================================

// Neighbor cache entries for ARP
static mut NEIGHBOR_CACHE_ENTRIES: [Option<(smoltcp::wire::IpAddress, smoltcp::iface::Neighbor)>; 8] = [None; 8];

// IP address storage - initialized at runtime
static mut IP_ADDRS: MaybeUninit<[IpCidr; 1]> = MaybeUninit::uninit();

// Routes storage
static mut ROUTES_STORAGE: [Option<(IpCidr, smoltcp::iface::Route)>; 1] = [None; 1];

// Socket set storage (7 sockets)
static mut SOCKETS_ENTRIES: [SocketStorage<'static>; 7] = [SocketStorage::EMPTY; 7];

// TCP server (telnet) buffers
static mut TCP_SERVER_RX_DATA: [u8; 256] = [0; 256];
static mut TCP_SERVER_TX_DATA: [u8; 256] = [0; 256];

// UDP server (artnet) buffers
static mut UDP_SERVER_RX_DATA: [u8; 2048] = [0; 2048];
static mut UDP_SERVER_TX_DATA: [u8; 2048] = [0; 2048];
static mut UDP_SERVER_RX_METADATA: [UdpPacketMetadata; 32] = [UdpPacketMetadata::EMPTY; 32];
static mut UDP_SERVER_TX_METADATA: [UdpPacketMetadata; 32] = [UdpPacketMetadata::EMPTY; 32];

// Bitmap UDP buffers
static mut BITMAP_UDP_RX_DATA: [u8; 65536] = [0; 65536];
static mut BITMAP_UDP_TX_DATA: [u8; 64] = [0; 64];
static mut BITMAP_UDP_RX_META: [UdpPacketMetadata; 48] = [UdpPacketMetadata::EMPTY; 48];
static mut BITMAP_UDP_TX_META: [UdpPacketMetadata; 1] = [UdpPacketMetadata::EMPTY; 1];

// TFTP UDP buffers
static mut TFTP_UDP_RX_DATA: [u8; 1024] = [0; 1024];
static mut TFTP_UDP_TX_DATA: [u8; 128] = [0; 128];
static mut TFTP_UDP_RX_META: [UdpPacketMetadata; 4] = [UdpPacketMetadata::EMPTY; 4];
static mut TFTP_UDP_TX_META: [UdpPacketMetadata; 4] = [UdpPacketMetadata::EMPTY; 4];

// HTTP TCP socket A buffers
static mut HTTP_TCP_RX_A: [u8; 512] = [0; 512];
static mut HTTP_TCP_TX_A: [u8; 2048] = [0; 2048];

// HTTP TCP socket B buffers
static mut HTTP_TCP_RX_B: [u8; 512] = [0; 512];
static mut HTTP_TCP_TX_B: [u8; 2048] = [0; 2048];

// The Interface itself (using UnsafeCell for interior mutability from ISR)
static mut IFACE: MaybeUninit<Interface<'static, Eth>> = MaybeUninit::uninit();
static mut IFACE_INITIALIZED: bool = false;

// Socket handles
static mut TCP_SERVER_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut UDP_SERVER_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut BITMAP_UDP_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut TFTP_UDP_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut DHCP_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut HTTP_HANDLE_A: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut HTTP_HANDLE_B: MaybeUninit<SocketHandle> = MaybeUninit::uninit();

// Bitmap receiver state
static mut BITMAP_RX: MaybeUninit<BitmapReceiver> = MaybeUninit::uninit();

// TFTP config loader state
static mut TFTP_LOADER: MaybeUninit<TftpConfigLoader> = MaybeUninit::uninit();

// HTTP server state
static mut HTTP_REQUESTS: [MaybeUninit<HttpRequest>; 2] = [MaybeUninit::uninit(), MaybeUninit::uninit()];
static mut HTTP_RESPONSES: [MaybeUninit<HttpResponse>; 2] = [MaybeUninit::uninit(), MaybeUninit::uninit()];
static mut HTTP_RESPONSE_SENT: [usize; 2] = [0; 2];
static mut HTTP_CLOSE_AT: [i64; 2] = [0; 2];
static mut HTTP_CONNECTED_AT: [i64; 2] = [0; 2];

// Telnet state
static mut TELNET_ACTIVE: bool = false;
static mut IAC_STATE: u8 = 0;

// Time tracking
static mut TIME_MS: i64 = 0;
static mut LAST_BITMAP_PACKET_MS: i64 = 0;

// DHCP configuration storage
static mut DHCP_CONFIGURED: bool = false;
static mut BOOT_SERVER: Option<([u8; 4], crate::menu::BootServerSource)> = None;
static mut TFTP_STARTED: bool = false;

// Pointers to main-loop owned resources (set during init)
static mut HUB75_PTR: *mut Hub75 = core::ptr::null_mut();
static mut LAYOUT_PTR: *mut LayoutConfig = core::ptr::null_mut();
static mut ANIMATION_PTR: *mut crate::menu::Animation = core::ptr::null_mut();
static mut BITMAP_STATS_PTR: *mut BitmapStats = core::ptr::null_mut();
static mut MENU_RUNNER_PTR: *mut u8 = core::ptr::null_mut(); // Actually *mut menu::Runner but we avoid generics
static mut MAC_BYTES: [u8; 6] = [0; 6];

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the network stack with static storage.
/// Must be called exactly once during startup, before enabling interrupts.
///
/// Returns the SocketHandle for the bitmap UDP socket (for binding).
pub fn init(
    ethmac: pac::Ethmac,
    ethmem: pac::Ethmem,
    mac_bytes: [u8; 6],
    hub75: *mut Hub75,
    layout: *mut LayoutConfig,
    animation: *mut crate::menu::Animation,
    bitmap_stats: *mut BitmapStats,
) {
    unsafe {
        // Store pointers to main-loop resources
        HUB75_PTR = hub75;
        LAYOUT_PTR = layout;
        ANIMATION_PTR = animation;
        BITMAP_STATS_PTR = bitmap_stats;
        MAC_BYTES = mac_bytes;

        // Initialize IP addresses array
        IP_ADDRS.write([IpCidr::new(IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), 0)]);

        // Create Eth device
        let device = Eth::new(ethmac, ethmem);

        // Create neighbor cache
        let neighbor_cache = NeighborCache::new(&mut NEIGHBOR_CACHE_ENTRIES[..]);

        // Create routes
        let routes = Routes::new(&mut ROUTES_STORAGE[..]);

        // Build the interface
        let iface = InterfaceBuilder::new(device, &mut SOCKETS_ENTRIES[..])
            .hardware_addr(EthernetAddress::from_bytes(&mac_bytes).into())
            .neighbor_cache(neighbor_cache)
            .ip_addrs(&mut IP_ADDRS.assume_init_mut()[..])
            .routes(routes)
            .finalize();

        IFACE.write(iface);
        IFACE_INITIALIZED = true;

        let iface = IFACE.assume_init_mut();

        // Create and add TCP server socket (telnet, port 23)
        let tcp_rx_buffer = TcpSocketBuffer::new(&mut TCP_SERVER_RX_DATA[..]);
        let tcp_tx_buffer = TcpSocketBuffer::new(&mut TCP_SERVER_TX_DATA[..]);
        let tcp_server_socket = TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer);
        TCP_SERVER_HANDLE.write(iface.add_socket(tcp_server_socket));

        // Create and add UDP server socket (artnet, port 6454)
        let udp_rx_buffer = UdpSocketBuffer::new(&mut UDP_SERVER_RX_METADATA[..], &mut UDP_SERVER_RX_DATA[..]);
        let udp_tx_buffer = UdpSocketBuffer::new(&mut UDP_SERVER_TX_METADATA[..], &mut UDP_SERVER_TX_DATA[..]);
        let udp_server_socket = UdpSocket::new(udp_rx_buffer, udp_tx_buffer);
        UDP_SERVER_HANDLE.write(iface.add_socket(udp_server_socket));

        // Create and add bitmap UDP socket (port 7000)
        let bitmap_rx = UdpSocketBuffer::new(&mut BITMAP_UDP_RX_META[..], &mut BITMAP_UDP_RX_DATA[..]);
        let bitmap_tx = UdpSocketBuffer::new(&mut BITMAP_UDP_TX_META[..], &mut BITMAP_UDP_TX_DATA[..]);
        let bitmap_udp_socket = UdpSocket::new(bitmap_rx, bitmap_tx);
        BITMAP_UDP_HANDLE.write(iface.add_socket(bitmap_udp_socket));

        // Create and add TFTP UDP socket
        let tftp_rx = UdpSocketBuffer::new(&mut TFTP_UDP_RX_META[..], &mut TFTP_UDP_RX_DATA[..]);
        let tftp_tx = UdpSocketBuffer::new(&mut TFTP_UDP_TX_META[..], &mut TFTP_UDP_TX_DATA[..]);
        let tftp_udp_socket = UdpSocket::new(tftp_rx, tftp_tx);
        TFTP_UDP_HANDLE.write(iface.add_socket(tftp_udp_socket));

        // Create and add DHCP socket
        let dhcp_socket = Dhcpv4Socket::new();
        DHCP_HANDLE.write(iface.add_socket(dhcp_socket));

        // Create and add HTTP TCP sockets
        let http_rx_a = TcpSocketBuffer::new(&mut HTTP_TCP_RX_A[..]);
        let http_tx_a = TcpSocketBuffer::new(&mut HTTP_TCP_TX_A[..]);
        let http_tcp_socket_a = TcpSocket::new(http_rx_a, http_tx_a);
        HTTP_HANDLE_A.write(iface.add_socket(http_tcp_socket_a));

        let http_rx_b = TcpSocketBuffer::new(&mut HTTP_TCP_RX_B[..]);
        let http_tx_b = TcpSocketBuffer::new(&mut HTTP_TCP_TX_B[..]);
        let http_tcp_socket_b = TcpSocket::new(http_rx_b, http_tx_b);
        HTTP_HANDLE_B.write(iface.add_socket(http_tcp_socket_b));

        // Bind bitmap UDP socket so poll() routes packets to its buffer
        {
            let socket = iface.get_socket::<UdpSocket>(*BITMAP_UDP_HANDLE.assume_init_ref());
            socket.bind(7000).ok();
        }

        // Initialize bitmap receiver
        BITMAP_RX.write(BitmapReceiver::new());

        // Initialize TFTP loader
        TFTP_LOADER.write(TftpConfigLoader::new());

        // Initialize HTTP state
        HTTP_REQUESTS[0].write(HttpRequest::new());
        HTTP_REQUESTS[1].write(HttpRequest::new());
        HTTP_RESPONSES[0].write(HttpResponse::new());
        HTTP_RESPONSES[1].write(HttpResponse::new());
    }
}

/// Update time from main loop's timer tick.
/// Call this from main loop when timer fires.
#[inline]
pub fn update_time_ms(ms: i64) {
    unsafe {
        TIME_MS = ms;
    }
}

/// Get the current time_ms value.
pub fn time_ms() -> i64 {
    unsafe { TIME_MS }
}

/// Check if currently streaming (for main loop to skip animation).
pub fn is_streaming() -> bool {
    const BOOT_GRACE_PERIOD_MS: i64 = 10_000;
    unsafe {
        TIME_MS > BOOT_GRACE_PERIOD_MS && TIME_MS - LAST_BITMAP_PACKET_MS < 200
    }
}

/// Get the boot server info (for display in status page).
pub fn boot_server() -> Option<([u8; 4], crate::menu::BootServerSource)> {
    unsafe { BOOT_SERVER }
}

/// Get bitmap stats for display.
pub fn bitmap_stats() -> BitmapStats {
    unsafe {
        if !BITMAP_STATS_PTR.is_null() {
            *BITMAP_STATS_PTR
        } else {
            BitmapStats::new()
        }
    }
}

/// Get MAC error counters from hardware.
pub fn mac_errors() -> (u32, u32, u32) {
    unsafe {
        if IFACE_INITIALIZED {
            IFACE.assume_init_ref().device().mac_errors()
        } else {
            (0, 0, 0)
        }
    }
}

/// Get IP address for HTTP status page.
pub fn ip_addr() -> [u8; 4] {
    unsafe {
        if IFACE_INITIALIZED {
            match IFACE.assume_init_ref().ip_addrs()[0].address() {
                IpAddress::Ipv4(v4) => v4.0,
                _ => [0u8; 4],
            }
        } else {
            [0u8; 4]
        }
    }
}

// ============================================================================
// Network Handler - Called from ISR
// ============================================================================

/// Main network handler called from ISR.
/// Processes all incoming packets and socket events.
///
/// This function does:
/// - iface.poll() to process incoming packets
/// - DHCP socket handling
/// - HTTP socket handling
/// - Telnet socket handling
/// - Artnet UDP handling
/// - Bitmap UDP handling
#[no_mangle]
pub extern "C" fn network_handler() {
    unsafe {
        if !IFACE_INITIALIZED {
            return;
        }

        let time = Instant::from_millis(TIME_MS);
        let iface = IFACE.assume_init_mut();

        // Poll the interface to process packets
        iface.poll(time).ok();

        // Handle DHCP
        handle_dhcp(iface);

        // Handle TFTP config loading
        handle_tftp(iface);

        // Handle Telnet (TCP port 23)
        handle_telnet(iface);

        // Handle Artnet (UDP port 6454)
        handle_artnet(iface);

        // Handle Bitmap UDP (port 7000) - process any packets that went through smoltcp
        handle_bitmap_smoltcp(iface);

        // Handle HTTP (TCP port 80)
        handle_http(iface);

        // Poll again to send any responses
        iface.poll(time).ok();
    }
}

/// Handle DHCP events.
unsafe fn handle_dhcp(iface: &mut Interface<'static, Eth>) {
    let dhcp_handle = *DHCP_HANDLE.assume_init_ref();
    let socket = iface.get_socket::<Dhcpv4Socket>(dhcp_handle);

    if let Some(event) = socket.poll() {
        match event {
            Dhcpv4Event::Configured(config) => {
                DHCP_CONFIGURED = true;
                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::Ipv4(config.address);
                });
                if let Some(router) = config.router {
                    iface.routes_mut().add_default_ipv4_route(router).ok();
                }
                // Start TFTP config load if not already done
                if !TFTP_STARTED {
                    use crate::menu::BootServerSource;
                    let (server, source) = if let Some(ip) = config.tftp_server_name {
                        (ip, BootServerSource::Option66)
                    } else {
                        (Ipv4Address([10, 11, 6, 65]), BootServerSource::Fallback)
                    };
                    BOOT_SERVER = Some((server.0, source));

                    // Build MAC-based filename
                    let m = &MAC_BYTES;
                    let mut fname = [0u8; 21];
                    const HEX: &[u8; 16] = b"0123456789abcdef";
                    for i in 0..6 {
                        fname[i * 3] = HEX[(m[i] >> 4) as usize];
                        fname[i * 3 + 1] = HEX[(m[i] & 0xf) as usize];
                        if i < 5 { fname[i * 3 + 2] = b'-'; }
                    }
                    fname[17..21].copy_from_slice(b".yml");
                    let fname_str = core::str::from_utf8(&fname).unwrap_or("config.yml");

                    TFTP_LOADER.assume_init_mut().start(server, fname_str);
                    TFTP_STARTED = true;
                }
            }
            Dhcpv4Event::Deconfigured => {
                DHCP_CONFIGURED = false;
                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::new(IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), 0);
                });
                iface.routes_mut().remove_default_ipv4_route();
            }
        }
    }

    // Static IP fallback after 10 seconds
    if TIME_MS == 10_000 {
        if iface.ip_addrs()[0].address() == IpAddress::Ipv4(Ipv4Address::UNSPECIFIED) {
            let fallback = Ipv4Cidr::new(Ipv4Address([10, 11, 6, 250]), 24);
            iface.update_ip_addrs(|addrs| {
                addrs[0] = IpCidr::Ipv4(fallback);
            });
        }
    }
}

/// Handle TFTP config loading.
unsafe fn handle_tftp(iface: &mut Interface<'static, Eth>) {
    let tftp_loader = TFTP_LOADER.assume_init_mut();
    if !tftp_loader.is_active() {
        return;
    }

    let tftp_handle = *TFTP_UDP_HANDLE.assume_init_ref();
    let socket = iface.get_socket::<UdpSocket>(tftp_handle);

    if tftp_loader.poll(socket, TIME_MS) {
        // Config loaded - parse and apply layout
        if let Some(layout) = tftp_loader.parse_config() {
            if !HUB75_PTR.is_null() && !LAYOUT_PTR.is_null() {
                let hub75 = &mut *HUB75_PTR;
                let w = layout.virtual_width();
                let h = layout.virtual_height();
                layout.apply(hub75);
                *LAYOUT_PTR = layout;

                // Redraw at new virtual size
                let total = (w as u32) * (h as u32);
                hub75.set_img_param(w, total);
                hub75.write_img_data(0, crate::patterns::grid(w, h));
                hub75.swap_buffers();
            }
        }
    }
}

/// Handle Telnet (TCP port 23).
unsafe fn handle_telnet(iface: &mut Interface<'static, Eth>) {
    let tcp_handle = *TCP_SERVER_HANDLE.assume_init_ref();
    let socket = iface.get_socket::<TcpSocket>(tcp_handle);

    if !socket.is_open() {
        socket.listen(23).ok();
    }

    // Telnet handling is minimal in ISR - just keep socket open
    // Full menu handling would require too much state
    if !TELNET_ACTIVE && socket.is_active() {
        IAC_STATE = 0;
        TELNET_ACTIVE = true;
    }
    if !socket.is_active() {
        TELNET_ACTIVE = false;
    }

    // For now, just drain received data to prevent buffer overflow
    if socket.may_recv() {
        let mut buf = [0u8; 64];
        while socket.can_recv() {
            socket.recv_slice(&mut buf).ok();
        }
    }
}

/// Handle Artnet (UDP port 6454).
unsafe fn handle_artnet(iface: &mut Interface<'static, Eth>) {
    let udp_handle = *UDP_SERVER_HANDLE.assume_init_ref();
    let socket = iface.get_socket::<UdpSocket>(udp_handle);

    if !socket.is_open() {
        socket.bind(6454).ok();
    }

    if HUB75_PTR.is_null() {
        return;
    }
    let hub75 = &mut *HUB75_PTR;

    while let Ok((data, _endpoint)) = socket.recv() {
        if let Ok((offset, data)) = crate::artnet::packet2hub75(data) {
            let palette_offset = ((1 << 16) - 2) * 170;
            if offset >= palette_offset {
                hub75.set_palette((offset - palette_offset) as u8, data);
            }
        }
    }
}

/// Handle bitmap UDP packets that went through smoltcp.
unsafe fn handle_bitmap_smoltcp(iface: &mut Interface<'static, Eth>) {
    let bitmap_handle = *BITMAP_UDP_HANDLE.assume_init_ref();
    let socket = iface.get_socket::<UdpSocket>(bitmap_handle);

    if HUB75_PTR.is_null() {
        return;
    }
    let hub75 = &mut *HUB75_PTR;
    let bitmap_rx = BITMAP_RX.assume_init_mut();

    while let Ok((data, _endpoint)) = socket.recv() {
        LAST_BITMAP_PACKET_MS = TIME_MS;
        let complete = bitmap_rx.process_packet(data, hub75, TIME_MS);
        if complete {
            hub75.swap_buffers();
            hub75.set_mode(crate::hub75::OutputMode::FullColor);
            hub75.on();
            if !ANIMATION_PTR.is_null() {
                *ANIMATION_PTR = crate::menu::Animation::None;
            }
        }
        if !BITMAP_STATS_PTR.is_null() {
            *BITMAP_STATS_PTR = bitmap_rx.stats;
        }
    }
}

/// Handle HTTP (TCP port 80).
unsafe fn handle_http(iface: &mut Interface<'static, Eth>) {
    let http_handles = [
        *HTTP_HANDLE_A.assume_init_ref(),
        *HTTP_HANDLE_B.assume_init_ref(),
    ];

    let http_ip = match iface.ip_addrs()[0].address() {
        IpAddress::Ipv4(v4) => v4.0,
        _ => [0u8; 4],
    };

    for i in 0..2 {
        let socket = iface.get_socket::<TcpSocket>(http_handles[i]);
        let request = HTTP_REQUESTS[i].assume_init_mut();
        let response = HTTP_RESPONSES[i].assume_init_mut();

        // Recycle socket after graceful close
        if HTTP_CLOSE_AT[i] > 0 {
            if !socket.is_active() || TIME_MS >= HTTP_CLOSE_AT[i] {
                if socket.is_open() {
                    socket.abort();
                }
                HTTP_CLOSE_AT[i] = 0;
            }
        }

        if !socket.is_open() {
            request.reset();
            response.data.clear();
            HTTP_RESPONSE_SENT[i] = 0;
            HTTP_CONNECTED_AT[i] = 0;
            socket.listen(80).ok();
        }

        // Only process fully established connections
        // Skip if socket is still in handshake (is_active but not yet may_recv)
        if !socket.may_recv() && !socket.may_send() {
            // Not established yet or already closed - skip
            if !socket.is_active() {
                HTTP_CONNECTED_AT[i] = 0;
            }
            continue;
        }

        // Track connection time for timeout
        if socket.is_active() {
            if HTTP_CONNECTED_AT[i] == 0 {
                HTTP_CONNECTED_AT[i] = TIME_MS;
            }
            // Abort if request incomplete and timed out
            if !request.is_complete() && TIME_MS > 0 && TIME_MS - HTTP_CONNECTED_AT[i] > 5000 {
                socket.abort();
                HTTP_CONNECTED_AT[i] = 0;
                continue;
            }
        } else {
            HTTP_CONNECTED_AT[i] = 0;
        }

        // Receive request
        if socket.can_recv() && !request.is_complete() {
            let mut buf = [0u8; 128];
            if let Ok(n) = socket.recv_slice(&mut buf) {
                if request.feed(&buf[..n]) {
                    // Request complete - handle it
                    // Note: We need Context for handle_request, but we don't have it in ISR
                    // For now, just send a minimal response
                    handle_http_request(request, response, http_ip);
                    HTTP_RESPONSE_SENT[i] = 0;
                }
            }
        }

        // Send response
        if socket.can_send() && HTTP_RESPONSE_SENT[i] < response.data.len() {
            if let Ok(sent) = socket.send_slice(&response.data[HTTP_RESPONSE_SENT[i]..]) {
                HTTP_RESPONSE_SENT[i] += sent;
            }
            if HTTP_RESPONSE_SENT[i] >= response.data.len() && response.data.len() > 0 {
                socket.close();
                HTTP_CLOSE_AT[i] = TIME_MS + 50;
            }
        }
    }
}

/// Handle HTTP request in ISR context (limited functionality).
unsafe fn handle_http_request(req: &HttpRequest, resp: &mut HttpResponse, ip: [u8; 4]) {
    use core::fmt::Write;
    use crate::http::Method;

    match (req.method(), req.path()) {
        (Method::Get, "/") => {
            // HTML status page
            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n").ok();

            // Get display info
            let (w, len) = if !HUB75_PTR.is_null() {
                (*HUB75_PTR).get_img_param()
            } else {
                (0, 0)
            };
            let h = if w > 0 { len / w as u32 } else { 0 };

            write!(resp, r#"<!DOCTYPE html><html><head>
<meta charset=utf-8><title>Colorlight v{}</title>
<style>body{{font:14px system-ui;background:#0f0f17;color:#ccc;padding:24px}}
h1{{color:#fff;border-left:3px solid #4a4ae0;padding-left:12px}}
.c{{background:#181828;border:1px solid #252540;border-radius:8px;padding:14px;margin:10px 0}}
table{{width:100%}}td{{padding:3px 0}}td:first-child{{color:#7a7a9a}}td+td{{text-align:right}}</style></head>
<body><h1>Colorlight v{}</h1>
<div class=c><table>
<tr><td>IP</td><td>{}.{}.{}.{}</td></tr>
<tr><td>Display</td><td>{}x{}</td></tr>
<tr><td>Mode</td><td>ISR-driven</td></tr>
</table></div>
<div class=c><b>Endpoints:</b><br>
GET /api/status - JSON status<br>
POST /api/reboot - Reboot device
</div></body></html>"#,
                env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION"),
                ip[0], ip[1], ip[2], ip[3], w, h).ok();
        }
        (Method::Get, "/api/status") => {
            // JSON status
            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
            write!(resp, r#"{{"ip":"{}.{}.{}.{}","isr":true}}"#,
                ip[0], ip[1], ip[2], ip[3]).ok();
        }
        (Method::Post, "/api/irq/enable") => {
            // Enable ETHMAC interrupt
            // 1. Enable ETHMAC peripheral interrupt (event manager)
            crate::ethernet::enable_rx_interrupt();

            // 2. Set VexRiscv IRQ_MASK bit 2 (ETHMAC is IRQ #2)
            core::arch::asm!("csrw 0xBC0, {}", in(reg) (1u32 << 2));

            // 3. Enable machine external interrupts and global interrupt enable
            riscv::register::mie::set_mext();
            riscv::register::mstatus::set_mie();

            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
            resp.data.extend_from_slice(br#"{"ok":true,"interrupts_enabled":true}"#).ok();
        }
        (Method::Post, "/api/reboot") => {
            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
            resp.data.extend_from_slice(br#"{"ok":true,"rebooting":true}"#).ok();
            // Schedule reboot after response is sent
            // Can't do it here as we need to send response first
        }
        _ => {
            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\nNot Found").ok();
        }
    }
}

// ============================================================================
// Raw fast path for bitmap UDP
// ============================================================================

/// Check if a raw Ethernet frame is a UDP packet destined for the bitmap port (7000).
/// Layout: Ethernet(14) + IPv4(20, IHL=5) + UDP(8) = 42-byte header.
#[inline]
pub fn is_bitmap_udp(frame: &[u8]) -> bool {
    frame.len() >= 52 // 42 header + 10 bitmap header min
        && frame[12] == 0x08 && frame[13] == 0x00 // EtherType: IPv4
        && frame[14] & 0x0F == 5                   // IHL: 5 (no options)
        && frame[23] == 17                          // Protocol: UDP
        && frame[36] == 0x1B && frame[37] == 0x58   // UDP dst port: 7000
}

/// Process a raw bitmap UDP packet from hardware.
/// Called from ISR for the fast path.
pub fn process_raw_bitmap(frame: &[u8]) -> bool {
    unsafe {
        if HUB75_PTR.is_null() {
            return false;
        }

        LAST_BITMAP_PACKET_MS = TIME_MS;

        let hub75 = &mut *HUB75_PTR;
        let bitmap_rx = BITMAP_RX.assume_init_mut();

        // Skip Ethernet(14) + IP(20) + UDP(8) = 42 byte header
        let complete = bitmap_rx.process_packet(&frame[42..], hub75, TIME_MS);

        if complete {
            hub75.swap_buffers();
            hub75.set_mode(crate::hub75::OutputMode::FullColor);
            hub75.on();
            if !ANIMATION_PTR.is_null() {
                *ANIMATION_PTR = crate::menu::Animation::None;
            }
        }

        if !BITMAP_STATS_PTR.is_null() {
            *BITMAP_STATS_PTR = bitmap_rx.stats;
        }

        complete
    }
}
