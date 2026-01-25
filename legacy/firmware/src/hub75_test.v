// HUB75 test pattern for Colorlight 5A-75E V8.x
// Displays color bars on a single 64x64 or 64x32 panel connected to J1
// Use this to verify HUB75 wiring before attempting network firmware

`default_nettype none

module hub75_test (
    input  wire osc25m,     // 25MHz clock from PHY
    output wire led,        // Onboard LED (directly accent tied)
    output wire phy_resetn, // Keep PHY out of reset

    // HUB75 panel on J1
    output wire panel_r0,
    output wire panel_g0,
    output wire panel_b0,
    output wire panel_r1,
    output wire panel_g1,
    output wire panel_b1,
    output wire panel_a,
    output wire panel_b,
    output wire panel_c,
    output wire panel_d,
    output wire panel_e,    // For 1/32 scan (64 row panels)
    output wire panel_clk,
    output wire panel_lat,
    output wire panel_oe
);

    // Keep PHY running for clock
    assign phy_resetn = 1'b1;

    // Blink LED as heartbeat
    reg [24:0] led_counter = 0;
    always @(posedge osc25m) led_counter <= led_counter + 1;
    assign led = ~led_counter[24];

    // Panel timing parameters
    // For a 64x64 panel with 1/32 scan:
    // - 64 columns to shift out
    // - 32 row pairs (addressed by A,B,C,D,E)
    localparam COLS = 64;
    localparam ROWS = 32;  // Number of row address lines (for 64 rows, need 5 bits = 32 addresses)

    // State machine
    localparam S_SHIFT = 0;
    localparam S_LATCH = 1;
    localparam S_UNBLANK = 2;

    reg [1:0] state = S_SHIFT;
    reg [6:0] col = 0;
    reg [4:0] row = 0;
    reg [7:0] brightness = 0;
    reg [3:0] bit_plane = 0;

    // Clock divider (slow down for visibility during debug)
    reg [2:0] clk_div = 0;
    wire tick = (clk_div == 0);
    always @(posedge osc25m) clk_div <= clk_div + 1;

    // Output registers
    reg r0, g0, b0, r1, g1, b1;
    reg [4:0] addr;
    reg clk_out, lat, oe;

    // Color pattern generation (8 vertical color bars)
    wire [2:0] color_top = col[5:3];    // Top half color based on column
    wire [2:0] color_bot = col[5:3];    // Bottom half (same pattern)

    // Simple PWM for brightness (using bit_plane)
    wire show_top_r = color_top[0] & (brightness[7:4] > bit_plane);
    wire show_top_g = color_top[1] & (brightness[7:4] > bit_plane);
    wire show_top_b = color_top[2] & (brightness[7:4] > bit_plane);
    wire show_bot_r = color_bot[0] & (brightness[7:4] > bit_plane);
    wire show_bot_g = color_bot[1] & (brightness[7:4] > bit_plane);
    wire show_bot_b = color_bot[2] & (brightness[7:4] > bit_plane);

    always @(posedge osc25m) begin
        if (tick) begin
            case (state)
                S_SHIFT: begin
                    oe <= 1'b1;  // Blank during shift
                    lat <= 1'b0;

                    // Shift out pixel data
                    r0 <= show_top_r;
                    g0 <= show_top_g;
                    b0 <= show_top_b;
                    r1 <= show_bot_r;
                    g1 <= show_bot_g;
                    b1 <= show_bot_b;

                    // Clock pulse
                    clk_out <= 1'b1;

                    if (col == COLS - 1) begin
                        col <= 0;
                        state <= S_LATCH;
                    end else begin
                        col <= col + 1;
                    end
                end

                S_LATCH: begin
                    clk_out <= 1'b0;
                    lat <= 1'b1;  // Latch data
                    addr <= row;  // Set row address
                    state <= S_UNBLANK;
                end

                S_UNBLANK: begin
                    lat <= 1'b0;
                    oe <= 1'b0;   // Enable output (display row)

                    // Move to next row/bit_plane
                    if (row == ROWS - 1) begin
                        row <= 0;
                        if (bit_plane == 15) begin
                            bit_plane <= 0;
                            brightness <= brightness + 1;  // Slowly cycle brightness
                        end else begin
                            bit_plane <= bit_plane + 1;
                        end
                    end else begin
                        row <= row + 1;
                    end

                    state <= S_SHIFT;
                end
            endcase
        end else begin
            clk_out <= 1'b0;  // Clock low between ticks
        end
    end

    // Output assignments
    assign panel_r0 = r0;
    assign panel_g0 = g0;
    assign panel_b0 = b0;
    assign panel_r1 = r1;
    assign panel_g1 = g1;
    assign panel_b1 = b1;
    assign panel_a = addr[0];
    assign panel_b = addr[1];
    assign panel_c = addr[2];
    assign panel_d = addr[3];
    assign panel_e = addr[4];
    assign panel_clk = clk_out;
    assign panel_lat = lat;
    assign panel_oe = oe;

endmodule
