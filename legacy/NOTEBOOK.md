# Colorlight 5A-75E LED Panel Debugging Notebook

## Project Goal
Drive a HUB75 P2 128x64 LED panel via Colorlight 5A-75E FPGA board using UDP over Ethernet.

## Hardware
- Colorlight 5A-75E v8.0 FPGA board (Lattice ECP5-25F)
- HUB75 P2 128x64 LED panel (1/32 scan)
- Connected via Ethernet to 10.11.6.250

## What Works
1. **Ping works** - FPGA responds to ICMP at 10.11.6.250
2. **FPGA-internal panel writes work** - Startup fills show correct colors
3. **Network layer works** - MAC, IP, ICMP all functional

## What Doesn't Work
- **UDP packets don't reach udp_panel_writer** - `udp_source_valid` never goes high
- LED watchdog test: LED keeps blinking (should go solid if UDP arrives)

## Key Files
- `/firmware/colorlight-led-cube/fpga/top.v` - Top level, wires liteeth to udp_panel_writer
- `/firmware/colorlight-led-cube/fpga/liteeth_core.v` - Generated LiteEth UDP/IP stack
- `/firmware/colorlight-led-cube/fpga/udp_panel_writer.v` - Receives UDP, writes to panel
- `/firmware/colorlight-led-cube/fpga/ledpanel.v` - Panel driver with block RAM
- `/ledpanel.py` - Python library for sending pixels

## LiteEth Configuration (in liteeth_core.v)
- **IP Address**: 10.11.6.250 (32'd168494842) - Line 4941
- **MAC Address**: AA:00:00:4E:5E:4F (48'd186934156644303) - Line 2628
- **UDP Port Filter**: Currently 6000 (was changed from original) - Line 4339

## Data Flow (should be)
```
Python UDP packet → PHY → MAC → IP → UDP crossbar → udp_panel_writer
                                         ↓
                              udp_source_valid (NEVER FIRES!)
```

## Changes Made to liteeth_core.v from Original
1. IP changed: 192.168.178.50 → 10.11.6.250 (lines 2629, 3102, 4800, 4941)
2. Port changed: 6000 → 26177 → back to 6000 (line 4339)

## Bug Found and Fixed (but didn't solve the problem)
- Line 4339 had `13'd26177` but wire is 16-bit
- 13'd26177 truncates to 1601 (26177 % 8192)
- Fixed to `16'd26177`, but UDP still doesn't work
- Reverted to port 6000 for testing

## Key Code Paths in LiteEth

### IP Layer Validation (line 4941)
```verilog
ip_rx_valid <= ((((ip_rx_depacketizer_source_valid
    & (ip_rx_depacketizer_source_param_target_ip == 32'd168494842))  // IP check
    & (ip_rx_depacketizer_source_param_version == 3'd4))             // IPv4
    & (ip_rx_depacketizer_source_param_ihl == 3'd5))                 // Header len
    & (ip_rx_liteethipv4checksum_value == 1'd0));                    // Checksum OK
```

### Protocol Routing (line 3456-3466)
```verilog
case (ip_crossbar_sink_param_protocol)
    1'd1: sel0 <= 1'd1;   // ICMP → works (ping works)
    5'd17: sel0 <= 2'd2;  // UDP → should route here
    default: sel0 <= 1'd0; // drop
endcase
```

### UDP Port Routing (line 4338-4345)
```verilog
case (crossbar_sink_param_dst_port)
    16'd6000: liteethudpipcore_liteethudp_sel <= 1'd1;  // Route to user port
    default: liteethudpipcore_liteethudp_sel <= 1'd0;   // Drop
endcase
```

### Signal Chain
```
udp_source_valid = user_port_source_valid (line 1599)
                 = internal_port_source_valid (line 4326)
                 = crossbar_sink_valid (line 4357)
                 = rx_source_source_valid (line 4019)
```

## Current Test Code (udp_panel_writer.v)
Minimal test - just checks if udp_source_valid ever fires:
```verilog
assign udp_source_ready = 1'b1;  // Always ready
always @(posedge clock) begin
    watchdog <= watchdog + 1;
    led_reg <= watchdog[26];  // Blink when idle
    if (udp_source_valid) watchdog <= 0;  // Go solid on UDP
end
```

## Test Commands
```bash
# Build and flash
./docker_build.sh firmware && ./flash.sh network

# Test ping
ping 10.11.6.250

# Test UDP (port 6000)
python3 -c "
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
for i in range(100):
    s.sendto(b'test', ('10.11.6.250', 6000))
"
```

## Theories to Test
1. **Clock domain issue** - liteeth uses eth_rx_clk (from PHY) and sys_clk (from PLL)
2. **Reset timing** - top.v line 92: `.sys_reset(reset & ~phy_init_done)` - is this correct?
3. **UDP checksum** - Does Python UDP have valid checksum? (Should be handled by OS)
4. **Data width mismatch** - liteeth provides 8-bit data, port declared as 32-bit

## Original Working Protocol (from udp_panel_writer.v)
- Receives 8-bit data from liteeth (not 32-bit!)
- Assembles 4 bytes into 32-bit pixel: `data = {data[23:0], udp_source_data[7:0]}`
- Pixel format: `[31:18]=addr, [17:12]=R, [11:6]=G, [5:0]=B`
- Port check: `udp_source_dst_port[15:8] == 0x66` (for port 0x66xx)

## Debugging Log

### Test 1: Minimal UDP valid check (port 6000)
- Code: Just check if udp_source_valid ever fires, LED goes solid
- Result: LED blinking, panel blank
- Conclusion: udp_source_valid NEVER fires

### Test 2: IP layer debug (ip_mac_port_source_valid)
- Code: Added debug output from liteeth for ip_mac_port_source_valid
- Result with ping: LED goes solid ✓
- Result with UDP: LED stays solid ✓
- Conclusion: IP layer SEES UDP packets, problem is downstream in UDP routing

### Test 3: UDP depacketizer output (rx_source_source_valid)
- Code: debug_udp_rx_valid = rx_source_source_valid
- Result with UDP: LED blinking ✗
- Conclusion: UDP packets NOT reaching UDP depacketizer output
- Problem is between IP layer and UDP depacketizer

### Test 4: IP crossbar output to UDP (ip_port_source_valid)
- Code: debug_udp_rx_valid = ip_port_source_valid
- Result with UDP: LED blinking ✗
- Conclusion: IP crossbar NOT routing UDP packets to UDP path

### Test 5: IP depacketizer output (ip_rx_source_source_valid)
- Code: debug_udp_rx_valid = ip_rx_source_source_valid
- Result with UDP: LED blinking ✗
- Conclusion: IP depacketizer NOT outputting valid for UDP packets

### Test 6: IP validation (ip_rx_valid) with LATCH
- Code: debug_udp_rx_valid = ip_rx_valid, with latch to catch short pulses
- Result with ping: LED latched solid ✓
- Result with UDP only: LED latched solid ✓
- Conclusion: ip_rx_valid DOES fire for UDP - IP validation passes!

### Test 7: IP depacketizer output (ip_rx_source_source_valid) with LATCH
- Code: debug_udp_rx_valid = ip_rx_source_source_valid, with latch
- Result with UDP: LED latched solid ✓
- Conclusion: IP depacketizer output DOES fire for UDP

### Test 8: IP crossbar to UDP (ip_port_source_valid) with LATCH
- Code: debug_udp_rx_valid = ip_port_source_valid, with latch
- Result with UDP: LED latched solid ✓
- Conclusion: IP crossbar DOES route UDP to UDP path

### Test 9: UDP depacketizer output (rx_source_source_valid) with LATCH
- Code: debug_udp_rx_valid = rx_source_source_valid, with latch
- Result with UDP: LED latched solid ✓
- Conclusion: UDP depacketizer output DOES fire for UDP

### Test 10: After UDP port routing (internal_port_source_valid) with LATCH
- Code: debug_udp_rx_valid = internal_port_source_valid, with latch
- Result with UDP: LED latched solid ✓
- Conclusion: UDP port routing WORKS - packets get through to user port

### Test 11: Final output (udp_source_valid) with LATCH
- Code: debug_udp_rx_valid = udp_source_valid, with latch
- Result with UDP: LED latched solid ✓
- Conclusion: udp_source_valid DOES FIRE! The entire UDP path works!

## ROOT CAUSE FOUND
The original watchdog test reset on each pulse, but pulses are SHORT.
Between pulses, watchdog incremented and LED blinked.
The UDP path is FULLY FUNCTIONAL - we just weren't detecting it properly.

### Test 12: UDP-triggered fill (udp_seen latch -> fill RED)
- Code: When udp_source_valid seen, fill panel with RED using internal counter
- Result: LED solid (fill completed), but panel BLANK
- Conclusion: Fill loop ran, but panel didn't show color

### Test 13: Startup fill (no UDP, fill GREEN at boot)
- Code: Fill panel with GREEN immediately at startup
- Result: Panel GREEN, LED solid ✓
- Conclusion: Panel write path WORKS at startup

### Test 14: Startup GREEN, then UDP-triggered RED fill
- Code: Fill GREEN at startup, then when UDP seen, fill RED (reusing fill_counter)
- Result: Panel stays GREEN, LED solid
- Conclusion: Fill loop ran but panel didn't change - counter reuse issue?

### Test 15: Separate counters for init and UDP fill
- Result: Panel stays GREEN, LED solid
- Conclusion: Same problem - fill loop runs but panel doesn't change

### Test 16: UDP-triggered fill only (no startup fill)
- Code: No startup fill, just fill RED when UDP seen
- Initial state: Panel blank, LED off (fill_done=0)
- After UDP send: LED solid ON (fill_done=1), panel BLANK
- Conclusion: Fill loop runs but panel doesn't show color

## CRITICAL OBSERVATION
- Startup fill works (panel shows color)
- UDP-triggered fill runs (LED shows completion) but panel stays blank
- Same ctrl_en/ctrl_addr/ctrl_wdat code, different trigger timing

### Test 17: Continuous fill after UDP (not one-shot)
- Code: Keep filling RED continuously after udp_seen (no fill_done flag)
- Result: LED solid, panel BLANK
- Conclusion: Even continuous fill doesn't work after UDP trigger

### Test 18: Delay before fill after UDP
- Code: Wait ~134ms after UDP seen, then fill RED
- Result: LED off, panel blank - UDP not detected
- Changed to use debug_udp_rx_valid instead of udp_source_valid
- Still no detection

### Test 19: Init GREEN + UDP triggers RED (with debug_udp_rx_valid)
- Code: Fill GREEN at startup, then fill RED after udp_seen latch
- Result: Panel GREEN, LED OFF initially
- After UDP: LED ON, panel stays GREEN
- Conclusion: UDP detected (LED ON) but RED fill not working

### Test 20: Continuous fill, color depends on udp_seen (CRITICAL)
- Code: Always fill (ctrl_en always 1), color = RED if udp_seen else GREEN
- No state machine, simplest possible test
- Result: Panel GREEN, LED OFF initially
- After UDP: **LED ON, panel mostly GREEN with ~20x20 RED in lower right corner**
- Conclusion: udp_seen=1 (LED proves it) but panel mostly GREEN

## CRITICAL OBSERVATION (Test 20)
- LED ON proves udp_seen = 1 RIGHT NOW
- ctrl_wdat SHOULD be RED (because udp_seen=1)
- But panel is mostly GREEN with only corner RED
- This is IMPOSSIBLE unless:
  1. Reset is firing intermittently (clearing udp_seen)
  2. Synthesis bug (udp_seen controls LED but not ctrl_wdat)
  3. Clock domain issue

## KEY INSIGHT
The watchdog-based detection was failing because signals are SHORT PULSES, not sustained.
Added a LATCH in udp_panel_writer to catch any pulse:
```verilog
if (debug_udp_rx_valid)
    debug_latch <= 1'b1;  // Once set, stays set
if (debug_latch)
    led_reg <= 1'b0;  // Solid ON
else
    led_reg <= watchdog[26];  // Blink
```

## Continued Testing (Session 2)

### Test 21: Permanent latch (no reset clearing)
- Code: udp_seen_permanent that only reset can clear
- Result: Same problem - mostly BLUE, small RED rectangle in corner

### Test 22: No reset at all in udp_seen logic
- Code: Removed reset from udp_seen entirely
- Result: Same problem

### Test 23: Timer-based switch (no UDP, no external signal)
- Code: After 1 second, switch from BLUE to RED
- Result: **WORKS PERFECTLY** - Full RED after timer expires
- Conclusion: The fill loop and panel write path are FINE

### Test 24: Synchronized UDP with stability wait
- Code: Wait for stable (1 sec), then check UDP with edge detection
- Result: Same problem - mostly BLUE, RED corner

### Test 25: Separate always blocks
- Code: Timer/latch in one block, fill logic in another
- Result: Same problem

### Test 26: LED shows when BLUE is being written
- Code: LED ON when ctrl_wdat == BLUE, OFF when RED
- Result: LED ON when panel is BLUE, proves color logic works

### Test 27: Button instead of UDP
- Code: Use button input to trigger color change
- Result: Mostly RED, some BLUE (better than UDP but still not perfect)

### Test 28: Button without synchronizer
- Code: Direct button input, no sync
- Result: Mostly RED

### Test 29: Button active high at startup (not pressed)
- Code: Check button == 1 (active high, not pressed at boot)
- Result: **FULLY RED** - Works perfectly!

### Test 30: UDP without synchronizer
- Code: Direct debug_udp_rx_valid, no sync
- Result: Mostly BLUE, LED OFF

### Test 31: UDP with edge detection
- Code: Edge detect on debug_udp_rx_valid
- Result: Mostly BLUE, 16x24 RED rectangle in corner

## CRITICAL FINDINGS

### What WORKS:
1. Timer-based color switch → Full RED ✓
2. Button HIGH at startup → Full RED ✓
3. Startup fill → Full GREEN ✓

### What PARTIALLY WORKS:
1. Button press (toggle) → Mostly RED, some BLUE

### What FAILS:
1. UDP trigger → Mostly BLUE, 16x24 RED rectangle in corner (~384 pixels)

### Key Observations:
1. **Rectangle is ALWAYS consistent** - same corner, same size (~16x24 = 384 pixels)
2. **User is always pinging** - liteeth is always active during tests
3. **LED proves latch works** - udp_seen goes HIGH and stays HIGH
4. **But panel doesn't respond correctly** - ctrl_wdat should be RED but panel stays mostly BLUE

### User Insight:
> "liteeth cannot be the issue as even the button does not work correctly"

## Theories

1. **Timing/glitch on external signals** - External signals (UDP, button) may cause glitches that corrupt address counter or ctrl_en
2. **Address counter corruption** - The 384-pixel RED rectangle suggests counter runs for ~384 cycles with correct color before something resets it
3. **Interaction with liteeth clock domain** - Even though we synchronize, liteeth's constant activity (from pings) may affect something
4. **Metastability** - Despite synchronizer, external signals crossing clock domains may cause unpredictable behavior

## Current Code (udp_panel_writer.v)
```verilog
assign ctrl_wr = 4'b0111;
assign udp_source_ready = 1'b1;

reg [15:0] fill_counter = 16'b0;
reg [27:0] timer = 28'b0;
reg stable = 1'b0;
reg udp_seen = 1'b0;
reg udp_prev = 1'b0;  // For edge detection

always @(posedge clock) begin
    timer <= timer + 1;
    if (timer == 28'd125000000)
        stable <= 1'b1;

    udp_prev <= debug_udp_rx_valid;

    if (stable && debug_udp_rx_valid && !udp_prev)
        udp_seen <= 1'b1;

    ctrl_en <= 6'b000001;
    ctrl_addr <= fill_counter[12:0];
    fill_counter <= fill_counter + 1;

    if (!stable) begin
        ctrl_wdat <= 24'h00FF00;  // GREEN
        led_reg <= 1'b1;
    end
    else if (udp_seen) begin
        ctrl_wdat <= 24'hFF0000;  // RED
        led_reg <= 1'b1;
    end
    else begin
        ctrl_wdat <= 24'h0000FF;  // BLUE
        led_reg <= 1'b0;
    end
end
```

## Next Investigation
1. Check if fill_counter is being corrupted by external signals
2. Try isolating udp_seen completely from liteeth domain
3. Add debug output for fill_counter value when UDP arrives
4. Test if the 384-pixel count correlates with any timing (384 cycles at 125MHz = 3.07µs)
