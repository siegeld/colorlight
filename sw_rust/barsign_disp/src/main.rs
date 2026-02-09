#![no_std]
#![no_main]

use core::fmt::Write as _;

use barsign_disp::*;
use embedded_hal::blocking::serial::Write;
use embedded_hal::serial::Read;
use hal::*;
use layout::LayoutConfig;
use litex_pac as pac;
use riscv_rt::entry;

// ============================================================================
// VexRiscv External Interrupt Handler
// ============================================================================
// VexRiscv interrupt setup:
// 1. Write trap handler address to mtvec (WRITE_ONLY - reads return 0)
// 2. Set bit N in CSR 0xBC0 (IRQ_MASK) to enable IRQ N
// 3. Set mie.MEIE and mstatus.MIE
//
// The ISR calls network_handler() which does ALL network processing:
// - iface.poll() to process packets
// - DHCP, HTTP, Telnet, Artnet, Bitmap handling
// Main loop does ZERO network code - only display/animation.

// Assembly trap vector - save ALL GPRs, call network_handler(), restore
// 31 registers (skip x0) = 124 bytes, round to 128 for alignment
core::arch::global_asm!(r#"
.section .text.trap_handler
.global _trap_handler
.align 4
_trap_handler:
    # Save all GPRs except x0 (128 bytes for 8-byte alignment)
    addi sp, sp, -128
    sw ra,  0(sp)     # x1
    # x2 (sp) not saved - we're using it
    sw gp,  4(sp)     # x3
    sw tp,  8(sp)     # x4
    sw t0, 12(sp)     # x5
    sw t1, 16(sp)     # x6
    sw t2, 20(sp)     # x7
    sw s0, 24(sp)     # x8
    sw s1, 28(sp)     # x9
    sw a0, 32(sp)     # x10
    sw a1, 36(sp)     # x11
    sw a2, 40(sp)     # x12
    sw a3, 44(sp)     # x13
    sw a4, 48(sp)     # x14
    sw a5, 52(sp)     # x15
    sw a6, 56(sp)     # x16
    sw a7, 60(sp)     # x17
    sw s2, 64(sp)     # x18
    sw s3, 68(sp)     # x19
    sw s4, 72(sp)     # x20
    sw s5, 76(sp)     # x21
    sw s6, 80(sp)     # x22
    sw s7, 84(sp)     # x23
    sw s8, 88(sp)     # x24
    sw s9, 92(sp)     # x25
    sw s10, 96(sp)    # x26
    sw s11, 100(sp)   # x27
    sw t3, 104(sp)    # x28
    sw t4, 108(sp)    # x29
    sw t5, 112(sp)    # x30
    sw t6, 116(sp)    # x31
    # 120-124 = padding

    # === ISR LOGIC ===
    # Increment counter at 0x40020000
    li t0, 0x40020000
    lw t1, 0(t0)
    addi t1, t1, 1
    sw t1, 0(t0)

    # Disable ev_enable to prevent interrupt storm
    li t0, 0xF0001814
    sw zero, 0(t0)

    # Call Rust network handler (does ALL network processing)
    call network_handler
    # === END ISR ===

    # Restore all GPRs
    lw ra,  0(sp)
    lw gp,  4(sp)
    lw tp,  8(sp)
    lw t0, 12(sp)
    lw t1, 16(sp)
    lw t2, 20(sp)
    lw s0, 24(sp)
    lw s1, 28(sp)
    lw a0, 32(sp)
    lw a1, 36(sp)
    lw a2, 40(sp)
    lw a3, 44(sp)
    lw a4, 48(sp)
    lw a5, 52(sp)
    lw a6, 56(sp)
    lw a7, 60(sp)
    lw s2, 64(sp)
    lw s3, 68(sp)
    lw s4, 72(sp)
    lw s5, 76(sp)
    lw s6, 80(sp)
    lw s7, 84(sp)
    lw s8, 88(sp)
    lw s9, 92(sp)
    lw s10, 96(sp)
    lw s11, 100(sp)
    lw t3, 104(sp)
    lw t4, 108(sp)
    lw t5, 112(sp)
    lw t6, 116(sp)
    addi sp, sp, 128
    mret
"#);

extern "C" {
    fn _trap_handler();
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

    // Initialize ISR counter at 0x40020000 to zero
    unsafe {
        core::ptr::write_volatile(0x40020000 as *mut u32, 0);
    }

    // Set up trap handler infrastructure (using main stack, no mscratch)
    unsafe {
        // Write trap handler address to mtvec (WRITE_ONLY - reads return 0, but writes work)
        let trap_addr = _trap_handler as *const () as usize;
        core::arch::asm!("csrw mtvec, {}", in(reg) trap_addr);
        ethernet::set_trap_addr(trap_addr);

        // Read back mtvec for debugging (will be 0 due to WRITE_ONLY mode)
        let mtvec: usize;
        core::arch::asm!("csrr {}, mtvec", out(reg) mtvec);
        ethernet::set_debug_mtvec(mtvec);
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
        ring_overflow: 0,
    };

    let mut r = menu::Runner::new(&menu::ROOT_MENU, &mut buffer, context);

    // Initialize the network stack with static storage
    // Pass pointers to display state so ISR can access them
    network::init(
        peripherals.ethmac,
        peripherals.ethmem,
        mac_bytes,
        &mut r.context.hub75 as *mut _,
        &mut r.context.layout as *mut _,
        &mut r.context.animation as *mut _,
        &mut r.context.bitmap_stats as *mut _,
    );

    writeln!(r.context.output.serial, "Network stack initialized (ISR-driven)").ok();

    // Process any pending packets before enabling interrupts
    // This ensures clean state and handles any stale packets in hardware FIFO
    network::network_handler();

    // Enable ETHMAC interrupts immediately - ISR handles all network from boot
    unsafe {
        // 1. Enable ETHMAC peripheral interrupt
        ethernet::enable_rx_interrupt();

        // 2. Set VexRiscv IRQ_MASK bit 2 (ETHMAC is IRQ #2)
        core::arch::asm!("csrw 0xBC0, {}", in(reg) (1u32 << 2));

        // 3. Enable machine external interrupts and global interrupt enable
        riscv::register::mie::set_mext();
        riscv::register::mstatus::set_mie();
    }
    writeln!(r.context.output.serial, "Interrupts enabled").ok();

    let mut time_ms: i64 = 0;

    // Configure timer0 for periodic 1ms ticks (non-blocking)
    unsafe {
        let t = &*pac::Timer0::ptr();
        t.en().write(|w| w.bits(0));
        t.reload().write(|w| w.bits(40_000 - 1));  // 40MHz / 1000 = 40000 cycles per ms
        t.load().write(|w| w.bits(40_000 - 1));
        t.en().write(|w| w.bits(1));
        t.ev_pending().write(|w| w.bits(1));        // clear any pending event
    }

    // ========================================================================
    // MAIN LOOP - ZERO NETWORK CODE
    // All network processing happens in ISR via network_handler()
    // ========================================================================
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
            // Update network module's time
            network::update_time_ms(time_ms);
        }

        // Check if ISR fired and re-enable when idle
        ethernet::check_and_reenable_interrupt();

        // Skip processing on non-timer ticks or when streaming
        let streaming = network::is_streaming();
        if !timer_fired || (time_ms % 5 != 0) {
            continue;
        }

        // Update MAC error counters for display
        let (ovf, pre, crc) = network::mac_errors();
        r.context.mac_overflow = ovf;
        r.context.mac_preamble_err = pre;
        r.context.mac_crc_err = crc;
        r.context.ring_overflow = ethernet::ring_overflow_count() as u32;

        // Update boot server info from network module
        r.context.boot_server = network::boot_server();

        // Update animation at ~30fps (every 33ms), but skip during streaming
        if !streaming && time_ms % 33 == 0 {
            r.context.animation_tick();
        }

        // Handle serial input for menu
        if let Ok(data) = r.context.output.serial.read() {
            r.input_byte(if data == b'\n' { b'\r' } else { data });
        }
    }
}
