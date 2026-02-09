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

// ============================================================================
// Timer-based accurate timing
// ============================================================================
// Timer0 configured as free-running 1-second counter.
// We track seconds from ev_pending ticks, and read countdown value for sub-second.

/// Seconds counter - incremented each time timer wraps (every 1 second)
static mut TIMER_SECONDS: i64 = 0;

/// Timer reload value (1 second at 40MHz)
const TIMER_RELOAD: u32 = 40_000_000;

/// Cycles per millisecond
const CYCLES_PER_MS: u32 = 40_000;

/// Update timer seconds counter and compute current time in ms.
/// Called from ISR to ensure second boundaries aren't missed.
#[inline]
fn update_time_from_timer() {
    unsafe {
        let t = &*pac::Timer0::ptr();
        // Count seconds from ev_pending (1 second period)
        while t.ev_pending().read().bits() != 0 {
            t.ev_pending().write(|w| w.bits(1)); // clear
            TIMER_SECONDS += 1;
        }
        // Update TIME_MS from seconds + current countdown position
        t.update_value().write(|w| w.bits(1)); // latch current value
        let countdown = t.value().read().bits();
        let elapsed_in_period = (TIMER_RELOAD - 1 - countdown) / CYCLES_PER_MS;
        TIME_MS = TIMER_SECONDS * 1000 + elapsed_in_period as i64;
    }
}

/// Get current time in milliseconds.
#[inline]
pub fn get_time_ms() -> i64 {
    unsafe { TIME_MS }
}

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

        // Update TIME_MS by draining timer ticks
        update_time_from_timer();

        let iface = IFACE.assume_init_mut();
        let time = Instant::from_millis(TIME_MS);
        let mut had_non_bitmap = false;

        // Process all packets in hardware FIFO
        loop {
            let eth = iface.device();
            match eth.peek_rx() {
                Some(frame) if is_bitmap_udp(frame) => {
                    // Bitmap UDP: process directly (fast path)
                    process_raw_bitmap(frame);
                    eth.ack_rx();
                }
                Some(_) => {
                    // Non-bitmap: let smoltcp process
                    iface.poll(time).ok();
                    had_non_bitmap = true;
                }
                None => break,
            }
        }

        // Handle socket events if we had non-bitmap packets
        if had_non_bitmap {
            handle_dhcp(iface);
            handle_tftp(iface);
            handle_telnet(iface);
            handle_artnet(iface);
            handle_bitmap_smoltcp(iface);
            handle_http(iface);
            iface.poll(time).ok();
        }

        crate::ethernet::check_and_reenable_interrupt();
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

/// Handle HTTP request in ISR context.
unsafe fn handle_http_request(req: &HttpRequest, resp: &mut HttpResponse, ip: [u8; 4]) {
    use core::fmt::Write;
    use crate::http::Method;

    match (req.method(), req.path()) {
        (Method::Get, "/") => page_status(resp, ip),
        (Method::Get, "/api/status") => api_status(resp, ip),
        (Method::Get, "/api/layout") => api_layout_get(resp),
        (Method::Get, "/api/display") => api_display_get(resp),
        (Method::Get, "/api/bitmap/stats") => api_bitmap_stats(resp),
        (Method::Post, "/api/display/on") => api_display_on(resp),
        (Method::Post, "/api/display/off") => api_display_off(resp),
        (Method::Post, "/api/display/pattern") => api_display_pattern(req, resp),
        (Method::Post, "/api/reboot") => api_reboot(resp),
        _ => {
            resp.data.clear();
            resp.data.extend_from_slice(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\nNot Found").ok();
        }
    }
}

// ============================================================================
// HTTP Page and API Handlers
// ============================================================================

unsafe fn page_status(resp: &mut HttpResponse, ip: [u8; 4]) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: text/html;charset=utf-8\r\nConnection: close\r\n\r\n").ok();

    let m = &MAC_BYTES;
    let (w, len) = if !HUB75_PTR.is_null() { (*HUB75_PTR).get_img_param() } else { (0, 0) };
    let h = if w > 0 { len / w as u32 } else { 0 };
    let layout = if !LAYOUT_PTR.is_null() { &*LAYOUT_PTR } else { return; };
    let stats = if !BITMAP_STATS_PTR.is_null() { &*BITMAP_STATS_PTR } else { return; };
    let anim = if !ANIMATION_PTR.is_null() {
        match &*ANIMATION_PTR {
            crate::menu::Animation::None => "None",
            crate::menu::Animation::Rainbow { .. } => "Rainbow",
        }
    } else { "Unknown" };

    // Get hardware counters
    let (mac_ovf, mac_pre, mac_crc) = if IFACE_INITIALIZED {
        IFACE.assume_init_ref().device().mac_errors()
    } else { (0, 0, 0) };
    let ring_ovf = crate::ethernet::ring_overflow_count();
    let isr_count = crate::ethernet::isr_count();

    // Calculate FPS
    let avg = stats.avg_interval_ms;
    let fps = if avg > 0 { 1000 / avg } else { 0 };

    // Read CSRs for interrupt status
    let mstatus: u32;
    let mie: u32;
    let irq_mask: u32;
    core::arch::asm!("csrr {}, mstatus", out(reg) mstatus);
    core::arch::asm!("csrr {}, mie", out(reg) mie);
    core::arch::asm!("csrr {}, 0xBC0", out(reg) irq_mask);
    let mie_enabled = (mstatus & 0x8) != 0;
    let meie_enabled = (mie & 0x800) != 0;
    let ethmac_masked = (irq_mask & 0x4) != 0;

    // HTML head with professional dark theme
    write!(resp, "\
<!DOCTYPE html><html><head>\
<meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
<link rel=icon href='data:,'><title>Colorlight {}</title>\
<style>\
*{{margin:0;box-sizing:border-box}}\
body{{font:15px/1.5 system-ui,sans-serif;background:#0a0a0f;color:#c0c0c8;padding:24px}}\
h1{{font-size:22px;color:#e8e8f0;margin:0 0 20px;padding-left:12px;border-left:4px solid #5050d0}}\
.g{{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:16px}}\
.c{{background:#14141e;border:1px solid #202030;border-radius:8px;padding:16px}}\
.c h2{{font-size:11px;text-transform:uppercase;letter-spacing:1.5px;color:#6060a0;margin:0 0 12px;font-weight:600}}\
table{{width:100%;border-collapse:collapse}}\
td{{padding:4px 0;font-size:15px}}td:first-child{{color:#8080a0}}\
td+td{{text-align:right;color:#e0e0e8;font-variant-numeric:tabular-nums}}\
.ok{{color:#50d080}}.warn{{color:#f0a050}}.err{{color:#e05050}}\
select,button{{font:14px system-ui;background:#1c1c2c;color:#d0d0d8;border:1px solid #303048;padding:8px 14px;border-radius:6px}}\
button{{background:#4848b0;color:#fff;border:0;cursor:pointer}}button:hover{{background:#5858c0}}\
.mono{{font-family:monospace}}\
.ft{{margin-top:20px;text-align:center;font-size:13px;color:#4a4a6a}}\
a{{color:#7090d0;text-decoration:none}}\
</style></head><body>\
<h1>Colorlight v{} <span style='font-size:13px;color:#6a6a8a;font-weight:normal'>// Interrupt-Driven</span></h1><div class=g>",
        env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")).ok();

    // Network card
    write!(resp, "<div class=c><h2>Network</h2><table>\
<tr><td>MAC</td><td class=mono>{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}</td></tr>\
<tr><td>IPv4</td><td class=mono>{}.{}.{}.{}</td></tr>",
        m[0], m[1], m[2], m[3], m[4], m[5],
        ip[0], ip[1], ip[2], ip[3]).ok();
    if let Some((sip, source)) = BOOT_SERVER {
        let src = match source {
            crate::menu::BootServerSource::Option66 => "DHCP opt66",
            crate::menu::BootServerSource::Fallback => "fallback",
        };
        write!(resp, "<tr><td>TFTP Server</td><td class=mono>{}.{}.{}.{} <span style='color:#5a5a6a'>({})</span></td></tr>",
            sip[0], sip[1], sip[2], sip[3], src).ok();
    }
    write!(resp, "</table></div>").ok();

    // Display card
    write!(resp, "<div class=c><h2>Display</h2><table>\
<tr><td>Resolution</td><td>{}x{}</td></tr>\
<tr><td>Virtual Grid</td><td>{}x{} = {}x{}</td></tr>\
<tr><td>Panel Size</td><td>{}x{}</td></tr>\
<tr><td>Animation</td><td>{}</td></tr>\
</table></div>",
        w, h,
        layout.grid_cols, layout.grid_rows, layout.virtual_width(), layout.virtual_height(),
        layout.panel_width, layout.panel_height, anim).ok();

    // Interrupt Status card
    write!(resp, "<div class=c><h2>Interrupt Status</h2><table>\
<tr><td>ISR Count</td><td class='{}'>{}</td></tr>\
<tr><td>mstatus.MIE</td><td class='{}'>{}</td></tr>\
<tr><td>mie.MEIE</td><td class='{}'>{}</td></tr>\
<tr><td>IRQ_MASK[2]</td><td class='{}'>{}</td></tr>\
<tr><td>Mode</td><td class='ok'>ISR-driven</td></tr>\
</table></div>",
        if isr_count > 0 { "ok" } else { "warn" }, isr_count,
        if mie_enabled { "ok" } else { "err" }, if mie_enabled { "enabled" } else { "disabled" },
        if meie_enabled { "ok" } else { "err" }, if meie_enabled { "enabled" } else { "disabled" },
        if ethmac_masked { "ok" } else { "err" }, if ethmac_masked { "enabled" } else { "disabled" }).ok();

    // Streaming card
    write!(resp, "<div class=c><h2>Streaming</h2><table>\
<tr><td>Frames</td><td>{}</td></tr>\
<tr><td>Partial</td><td class='{}'>{}</td></tr>\
<tr><td>Dropped</td><td class='{}'>{}</td></tr>\
<tr><td>FPS</td><td>{} <span style='color:#5a5a6a'>({}ms avg)</span></td></tr>\
<tr><td>Jitter</td><td class='{}'>{}ms</td></tr>\
</table></div>",
        stats.frames_completed,
        if stats.frames_partial > 0 { "warn" } else { "" }, stats.frames_partial,
        if stats.frames_dropped > 0 { "err" } else { "" }, stats.frames_dropped,
        fps, avg,
        if stats.jitter_ms > 10 { "warn" } else { "" }, stats.jitter_ms).ok();

    // MAC Diagnostics card
    write!(resp, "<div class=c><h2>MAC Diagnostics</h2><table>\
<tr><td>RX Overflow</td><td class='{}'>{}</td></tr>\
<tr><td>CRC Errors</td><td class='{}'>{}</td></tr>\
<tr><td>Preamble Errors</td><td class='{}'>{}</td></tr>\
<tr><td>Ring Overflow</td><td class='{}'>{}</td></tr>\
</table></div>",
        if mac_ovf > 0 { "err" } else { "" }, mac_ovf,
        if mac_crc > 0 { "err" } else { "" }, mac_crc,
        if mac_pre > 0 { "warn" } else { "" }, mac_pre,
        if ring_ovf > 0 { "err" } else { "" }, ring_ovf).ok();

    // Panels card - only show J1 and J2 (first 2 outputs, 2 chain slots each)
    write!(resp, "<div class=c><h2>Panel Assignments</h2><table>").ok();
    for i in 0..2 {
        if i < layout.assignments.len() {
            for c in 0..2 {
                if c < layout.assignments[i].len() {
                    let label = match (i, c) {
                        (0, 0) => "J1[0]", (0, 1) => "J1[1]",
                        (1, 0) => "J2[0]", _ => "J2[1]",
                    };
                    match layout.assignments[i][c] {
                        Some((col, row)) => {
                            write!(resp, "<tr><td>{}</td><td>{},{}</td></tr>", label, col, row).ok();
                        }
                        None => {
                            write!(resp, "<tr><td>{}</td><td style='color:#404060'>-</td></tr>", label).ok();
                        }
                    }
                }
            }
        }
    }
    write!(resp, "</table></div>").ok();

    // Controls card
    write!(resp, "<div class=c><h2>Controls</h2>\
<div style='margin-bottom:8px'>\
<select id=p>\
<option>grid<option>rainbow<option>rainbow_anim\
<option>white<option>red<option>green<option>blue\
</select> \
<button onclick=\"fetch('/api/display/pattern',{{method:'POST',headers:{{'Content-Type':'application/json'}},\
body:JSON.stringify({{name:p.value}})}}).then(r=>r.json()).then(j=>{{m.textContent=j.ok?'OK':'Error'}})\
.catch(()=>{{m.textContent='Failed'}})\">Load</button> \
<span id=m style='color:#5a5a6a'></span></div>\
<button onclick=\"fetch('/api/reboot',{{method:'POST'}});this.textContent='Rebooting...';this.disabled=1\">Reboot</button>\
</div>").ok();

    // Footer
    write!(resp, "</div><div class=ft>\
<a href=/api/status>status</a> · \
<a href=/api/layout>layout</a> · \
<a href=/api/display>display</a> · \
<a href=/api/bitmap/stats>bitmap/stats</a>\
</div></body></html>").ok();
}

unsafe fn api_status(resp: &mut HttpResponse, ip: [u8; 4]) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();

    let m = &MAC_BYTES;
    let (w, len) = if !HUB75_PTR.is_null() { (*HUB75_PTR).get_img_param() } else { (0, 0) };
    let h = if w > 0 { len / w as u32 } else { 0 };
    let layout = if !LAYOUT_PTR.is_null() { &*LAYOUT_PTR } else { return; };
    let stats = if !BITMAP_STATS_PTR.is_null() { &*BITMAP_STATS_PTR } else { return; };
    let anim = if !ANIMATION_PTR.is_null() {
        match &*ANIMATION_PTR {
            crate::menu::Animation::None => "none",
            crate::menu::Animation::Rainbow { .. } => "rainbow",
        }
    } else { "unknown" };

    let isr_count = crate::ethernet::isr_count();
    let (mac_ovf, mac_pre, mac_crc) = if IFACE_INITIALIZED {
        IFACE.assume_init_ref().device().mac_errors()
    } else { (0, 0, 0) };

    write!(resp, r#"{{"mac":"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}","#,
        m[0], m[1], m[2], m[3], m[4], m[5]).ok();
    write!(resp, r#""ip":"{}.{}.{}.{}","#, ip[0], ip[1], ip[2], ip[3]).ok();
    write!(resp, r#""display_width":{},"display_height":{},"#, w, h).ok();
    write!(resp, r#""grid":"{}x{}","virtual_width":{},"virtual_height":{},"#,
        layout.grid_cols, layout.grid_rows, layout.virtual_width(), layout.virtual_height()).ok();
    write!(resp, r#""panel_width":{},"panel_height":{},"#, layout.panel_width, layout.panel_height).ok();
    write!(resp, r#""animation":"{}","bitmap_frames":{},"#, anim, stats.frames_completed).ok();
    write!(resp, r#""isr_count":{},"isr_driven":true,"#, isr_count).ok();
    write!(resp, r#""mac_overflow":{},"mac_crc_errors":{},"mac_preamble_errors":{}}}"#,
        mac_ovf, mac_crc, mac_pre).ok();
}

unsafe fn api_layout_get(resp: &mut HttpResponse) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();

    let layout = if !LAYOUT_PTR.is_null() { &*LAYOUT_PTR } else { return; };
    write!(resp, r#"{{"grid":"{}x{}","panel_width":{},"panel_height":{},"#,
        layout.grid_cols, layout.grid_rows, layout.panel_width, layout.panel_height).ok();
    write!(resp, r#""virtual_width":{},"virtual_height":{},"panels":{{"#,
        layout.virtual_width(), layout.virtual_height()).ok();
    let mut first = true;
    for (i, chain_slots) in layout.assignments.iter().enumerate() {
        let has_any = chain_slots.iter().any(|a| a.is_some());
        if has_any {
            if !first { write!(resp, ",").ok(); }
            write!(resp, r#""J{}":["#, i + 1).ok();
            let mut first_slot = true;
            for a in chain_slots.iter() {
                if !first_slot { write!(resp, ",").ok(); }
                match a {
                    Some((col, row)) => { write!(resp, r#""{},{}""#, col, row).ok(); }
                    None => { write!(resp, "null").ok(); }
                }
                first_slot = false;
            }
            write!(resp, "]").ok();
            first = false;
        }
    }
    write!(resp, "}}}}").ok();
}

unsafe fn api_display_get(resp: &mut HttpResponse) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();

    let (w, len) = if !HUB75_PTR.is_null() { (*HUB75_PTR).get_img_param() } else { (0, 0) };
    let h = if w > 0 { len / w as u32 } else { 0 };
    let mode = if !HUB75_PTR.is_null() {
        match (*HUB75_PTR).get_mode() {
            crate::hub75::OutputMode::FullColor => "fullcolor",
            crate::hub75::OutputMode::Indexed => "indexed",
        }
    } else { "unknown" };
    let anim = if !ANIMATION_PTR.is_null() {
        match &*ANIMATION_PTR {
            crate::menu::Animation::None => "none",
            crate::menu::Animation::Rainbow { .. } => "rainbow",
        }
    } else { "unknown" };
    write!(resp, r#"{{"width":{},"height":{},"mode":"{}","animation":"{}"}}"#, w, h, mode, anim).ok();
}

unsafe fn api_bitmap_stats(resp: &mut HttpResponse) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();

    let stats = if !BITMAP_STATS_PTR.is_null() { &*BITMAP_STATS_PTR } else { return; };
    let fps = if stats.avg_interval_ms > 0 { 1000 / stats.avg_interval_ms } else { 0 };
    let (mac_ovf, mac_pre, mac_crc) = if IFACE_INITIALIZED {
        IFACE.assume_init_ref().device().mac_errors()
    } else { (0, 0, 0) };
    let ring_ovf = crate::ethernet::ring_overflow_count();

    write!(resp, r#"{{"packets_total":{},"packets_valid":{},"#, stats.packets_total, stats.packets_valid).ok();
    write!(resp, r#""bad_magic":{},"bad_header":{},"#, stats.packets_bad_magic, stats.packets_bad_header).ok();
    write!(resp, r#""frames_completed":{},"frames_partial":{},"frames_dropped":{},"#,
        stats.frames_completed, stats.frames_partial, stats.frames_dropped).ok();
    write!(resp, r#""fps":{},"frame_interval_ms":{},"avg_interval_ms":{},"jitter_ms":{},"#,
        fps, stats.frame_interval_ms, stats.avg_interval_ms, stats.jitter_ms).ok();
    write!(resp, r#""last_frame_id":{},"#, stats.last_frame_id).ok();
    write!(resp, r#""last_chunk":"{}/{}","last_size":"{}x{}","last_data_len":{},"#,
        stats.last_chunk_index, stats.last_total_chunks,
        stats.last_width, stats.last_height, stats.last_data_len).ok();
    write!(resp, r#""mac_overflow":{},"mac_crc_errors":{},"mac_preamble_errors":{},"ring_overflow":{},"#,
        mac_ovf, mac_crc, mac_pre, ring_ovf).ok();
    write!(resp, r#""isr_count":{},"mtvec":"0x{:08x}","trap_addr":"0x{:08x}","time_ms":{}}}"#,
        crate::ethernet::isr_count(), crate::ethernet::debug_mtvec(), crate::ethernet::trap_addr(), TIME_MS).ok();
}

unsafe fn api_display_on(resp: &mut HttpResponse) {
    use core::fmt::Write;
    if !HUB75_PTR.is_null() { (*HUB75_PTR).on(); }
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
    write!(resp, r#"{{"ok":true,"display":"on"}}"#).ok();
}

unsafe fn api_display_off(resp: &mut HttpResponse) {
    use core::fmt::Write;
    if !HUB75_PTR.is_null() { (*HUB75_PTR).off(); }
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
    write!(resp, r#"{{"ok":true,"display":"off"}}"#).ok();
}

unsafe fn api_display_pattern(req: &HttpRequest, resp: &mut HttpResponse) {
    use core::fmt::Write;
    resp.data.clear();

    // Parse JSON body for "name" field (simple parser)
    let body = req.body_str();
    let name = match json_get_str(body, "name") {
        Some(n) => n,
        None => {
            resp.data.extend_from_slice(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nmissing \"name\"").ok();
            return;
        }
    };

    if HUB75_PTR.is_null() {
        resp.data.extend_from_slice(b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\nno display").ok();
        return;
    }
    let hub75 = &mut *HUB75_PTR;
    let (w, len) = hub75.get_img_param();
    let h = if w > 0 { (len / w as u32) as u16 } else { 0 };
    if w == 0 || h == 0 {
        resp.data.extend_from_slice(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nimage params not set").ok();
        return;
    }

    use crate::patterns;
    let mut anim = false;
    let ok = match name {
        "grid" => { hub75.write_img_data(0, patterns::grid(w, h)); true }
        "rainbow" => { hub75.write_img_data(0, patterns::rainbow(w, h)); true }
        "rainbow_anim" => {
            hub75.write_img_data(0, patterns::animated_rainbow(w, h, 0));
            anim = true;
            true
        }
        "white" => { hub75.write_img_data(0, patterns::solid_white(w, h)); true }
        "red" => { hub75.write_img_data(0, patterns::solid_red(w, h)); true }
        "green" => { hub75.write_img_data(0, patterns::solid_green(w, h)); true }
        "blue" => { hub75.write_img_data(0, patterns::solid_blue(w, h)); true }
        _ => false,
    };

    if ok {
        if !ANIMATION_PTR.is_null() {
            *ANIMATION_PTR = if anim {
                crate::menu::Animation::Rainbow { phase: 0 }
            } else {
                crate::menu::Animation::None
            };
        }
        hub75.swap_buffers();
        hub75.set_mode(crate::hub75::OutputMode::FullColor);
        hub75.on();
        resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
        write!(resp, r#"{{"ok":true,"pattern":"{}"}}"#, name).ok();
    } else {
        resp.data.extend_from_slice(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nunknown pattern").ok();
    }
}

unsafe fn api_reboot(resp: &mut HttpResponse) {
    use core::fmt::Write;
    resp.data.clear();
    resp.data.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").ok();
    write!(resp, r#"{{"ok":true,"rebooting":true}}"#).ok();
    // Actual reboot would be scheduled after response sent
}

/// Simple JSON string extractor for "key":"value" patterns
fn json_get_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let b = json.as_bytes();
    let kb = key.as_bytes();
    let len = b.len();
    let mut i = 0;
    while i < len {
        if b[i] == b'"' {
            let ks = i + 1;
            let ke = ks + kb.len();
            if ke < len && &b[ks..ke] == kb && b[ke] == b'"' {
                let mut p = ke + 1;
                while p < len && b[p] == b' ' { p += 1; }
                if p < len && b[p] == b':' {
                    p += 1;
                    while p < len && b[p] == b' ' { p += 1; }
                    if p < len && b[p] == b'"' {
                        let vs = p + 1;
                        let mut j = vs;
                        while j < len && b[j] != b'"' { j += 1; }
                        if j < len {
                            return core::str::from_utf8(&b[vs..j]).ok();
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
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

        // Update streaming timestamp on every packet
        LAST_BITMAP_PACKET_MS = TIME_MS;

        let hub75 = &mut *HUB75_PTR;
        let bitmap_rx = BITMAP_RX.assume_init_mut();

        // Skip Ethernet(14) + IP(20) + UDP(8) = 42 byte header
        let complete = bitmap_rx.process_packet(&frame[42..], hub75, TIME_MS);

        // Only do expensive operations on frame completion
        if complete {
            hub75.swap_buffers();
            hub75.set_mode(crate::hub75::OutputMode::FullColor);
            hub75.on();
            if !ANIMATION_PTR.is_null() {
                *ANIMATION_PTR = crate::menu::Animation::None;
            }
            if !BITMAP_STATS_PTR.is_null() {
                *BITMAP_STATS_PTR = bitmap_rx.stats;
            }
        }

        complete
    }
}
