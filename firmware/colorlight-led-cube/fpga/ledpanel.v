// Description of the LED panel:
// http://bikerglen.com/projects/lighting/led-panel-1up/#The_LED_Panel
//
// PANEL_[ABCD] ... select rows (in pairs from top and bottom half)
// PANEL_OE ....... display the selected rows (active low)
// PANEL_CLK ...... serial clock for color data
// PANEL_STB ...... latch shifted data (active high)
// PANEL_[RGB]0 ... color channel for top half
// PANEL_[RGB]1 ... color channel for bottom half
// taken from http://svn.clifford.at/handicraft/2015/c3demo/fpga/ledpanel.v
// modified by Niklas Fauth 2020
// modified for block RAM inference 2025

`default_nettype none
module ledpanel (
  input wire ctrl_clk,

	input wire ctrl_en,
	input wire [3:0] ctrl_wr,           // Which color memory block to write
	input wire [15:0] ctrl_addr,        // Addr to write color info on [col_info][row_info]
	input wire [23:0] ctrl_wdat,        // Data to be written [R][G][B]

	input wire display_clock,
	output reg panel_r0, panel_g0, panel_b0, panel_r1, panel_g1, panel_b1,
	output reg panel_a, panel_b, panel_c, panel_d, panel_e, panel_clk, panel_stb, panel_oe
);

  parameter integer INPUT_DEPTH          = 6;    // bits of color before gamma correction
  parameter integer COLOR_DEPTH          = 6;    // bits of color after gamma correction
  parameter integer CHAINED              = 2;    // number of 64-wide panels in chain (2 = 128x64)

  localparam integer SIZE_BITS = $clog2(CHAINED);
  localparam integer ADDR_WIDTH = 6 + 6 + SIZE_BITS;  // addr_y (6) + addr_x (6+SIZE_BITS)
  localparam integer MEM_SIZE = CHAINED * 4096;

  // Video memory - use synthesis attribute for block RAM
  (* ram_style = "block" *)
  reg [COLOR_DEPTH-1:0] video_mem_r [0:MEM_SIZE-1];
  (* ram_style = "block" *)
  reg [COLOR_DEPTH-1:0] video_mem_g [0:MEM_SIZE-1];
  (* ram_style = "block" *)
  reg [COLOR_DEPTH-1:0] video_mem_b [0:MEM_SIZE-1];

  // Gamma LUT - small, can stay as distributed/LUT ROM
  reg [COLOR_DEPTH-1:0] gamma_mem [0:2**INPUT_DEPTH-1];

  // Read address for video memory (display side)
  reg [ADDR_WIDTH-1:0] rd_addr;

  // Registered video memory outputs (required for block RAM inference)
  reg [COLOR_DEPTH-1:0] mem_r_out;
  reg [COLOR_DEPTH-1:0] mem_g_out;
  reg [COLOR_DEPTH-1:0] mem_b_out;

  // Gamma-corrected values
  reg [COLOR_DEPTH-1:0] gamma_r;
  reg [COLOR_DEPTH-1:0] gamma_g;
  reg [COLOR_DEPTH-1:0] gamma_b;

  // Initialize gamma LUT (small enough for synthesis)
  initial begin:init_block
    panel_a <= 0;
    panel_b <= 0;
    panel_c <= 0;
    panel_d <= 0;
    panel_e <= 0;
    $readmemh("6bit_to_7bit_gamma.mem", gamma_mem);
  end

  // Initialize video memory to zero (simpler for BRAM inference)
  integer i;
  initial begin
    for (i = 0; i < MEM_SIZE; i = i + 1) begin
      video_mem_r[i] = 0;
      video_mem_g[i] = 0;
      video_mem_b[i] = 0;
    end
  end

  // Write port - ctrl_clk domain
  always @(posedge ctrl_clk) begin
    if (ctrl_en && ctrl_wr[2]) video_mem_r[ctrl_addr[ADDR_WIDTH-1:0]] <= ctrl_wdat[16+INPUT_DEPTH-1:16];
    if (ctrl_en && ctrl_wr[1]) video_mem_g[ctrl_addr[ADDR_WIDTH-1:0]] <= ctrl_wdat[8+INPUT_DEPTH-1:8];
    if (ctrl_en && ctrl_wr[0]) video_mem_b[ctrl_addr[ADDR_WIDTH-1:0]] <= ctrl_wdat[0+INPUT_DEPTH-1:0];
  end

  // Display timing counters
  reg [5+COLOR_DEPTH+SIZE_BITS:0] cnt_x = 0;
  reg [4:0]                       cnt_y = 0;
  reg [2:0]                       cnt_z = 0;
  reg state = 0;

  reg [5+SIZE_BITS:0] addr_x;
  reg [5:0]           addr_y;
  reg [2:0]           addr_z;
  reg [2:0]           data_rgb;
  reg [2:0]           data_rgb_q;
  reg [5+COLOR_DEPTH+SIZE_BITS:0] max_cnt_x;

  always @(posedge display_clock) begin
    case (cnt_z)
      0: max_cnt_x <= 64*CHAINED+8;
      1: max_cnt_x <= 128*CHAINED;
      2: max_cnt_x <= 256*CHAINED;
      3: max_cnt_x <= 512*CHAINED;
      4: max_cnt_x <= 1024*CHAINED;
      5: max_cnt_x <= 2048*CHAINED;
      default: max_cnt_x <= 4096*CHAINED;
    endcase
  end

  always @(posedge display_clock) begin
    state <= !state;
    if (!state) begin
      if (cnt_x > max_cnt_x) begin
        cnt_x <= 0;
        cnt_z <= cnt_z + 1;
        if (cnt_z == COLOR_DEPTH-1) begin
          cnt_y <= cnt_y + 1;
          cnt_z <= 0;
        end
      end else begin
        cnt_x <= cnt_x + 1;
      end
    end
  end

  always @(posedge display_clock) begin
    panel_oe <= 64*CHAINED-8 < cnt_x && cnt_x < 64*CHAINED+8;
    if (state) begin
      panel_clk <= 1 < cnt_x && cnt_x < 64*CHAINED+2;
      panel_stb <= cnt_x == 64*CHAINED+2;
    end else begin
      panel_clk <= 0;
      panel_stb <= 0;
    end
  end

  // Stage 1: Compute address
  always @(posedge display_clock) begin
    addr_x <= cnt_x[5+SIZE_BITS:0];
    addr_y <= cnt_y + 32*(!state);
    addr_z <= cnt_z;
    rd_addr <= {cnt_y + 32*(!state), cnt_x[5+SIZE_BITS:0]};
  end

  // Stage 2: Read video memory (registered output for block RAM)
  always @(posedge display_clock) begin
    mem_r_out <= video_mem_r[rd_addr];
    mem_g_out <= video_mem_g[rd_addr];
    mem_b_out <= video_mem_b[rd_addr];
  end

  // Stage 3: Gamma lookup
  always @(posedge display_clock) begin
    gamma_r <= gamma_mem[mem_r_out];
    gamma_g <= gamma_mem[mem_g_out];
    gamma_b <= gamma_mem[mem_b_out];
  end

  // Stage 4: Bit selection for BCM
  reg [2:0] addr_z_d1, addr_z_d2, addr_z_d3;
  always @(posedge display_clock) begin
    addr_z_d1 <= addr_z;
    addr_z_d2 <= addr_z_d1;
    addr_z_d3 <= addr_z_d2;
    data_rgb[2] <= gamma_r[addr_z_d3];
    data_rgb[1] <= gamma_g[addr_z_d3];
    data_rgb[0] <= gamma_b[addr_z_d3];
  end

  // Delay state tracking to match pipeline
  reg [3:0] state_pipe;
  reg [5+COLOR_DEPTH+SIZE_BITS:0] cnt_x_d1, cnt_x_d2, cnt_x_d3, cnt_x_d4;
  reg [4:0] cnt_y_d1, cnt_y_d2, cnt_y_d3, cnt_y_d4;

  always @(posedge display_clock) begin
    state_pipe <= {state_pipe[2:0], state};
    cnt_x_d1 <= cnt_x; cnt_x_d2 <= cnt_x_d1; cnt_x_d3 <= cnt_x_d2; cnt_x_d4 <= cnt_x_d3;
    cnt_y_d1 <= cnt_y; cnt_y_d2 <= cnt_y_d1; cnt_y_d3 <= cnt_y_d2; cnt_y_d4 <= cnt_y_d3;
  end

  // Output stage - uses delayed signals to match pipeline
  always @(posedge display_clock) begin
    data_rgb_q <= data_rgb;
    if (!state_pipe[3]) begin
      if (0 < cnt_x_d4 && cnt_x_d4 < 64*CHAINED+1) begin
        {panel_r1, panel_r0} <= {data_rgb[2], data_rgb_q[2]};
        {panel_g1, panel_g0} <= {data_rgb[1], data_rgb_q[1]};
        {panel_b1, panel_b0} <= {data_rgb[0], data_rgb_q[0]};
      end else begin
        {panel_r1, panel_r0} <= 0;
        {panel_g1, panel_g0} <= 0;
        {panel_b1, panel_b0} <= 0;
      end
    end
    else if (cnt_x_d4 == 64*CHAINED) begin
      {panel_e, panel_d, panel_c, panel_b, panel_a} <= cnt_y_d4;
    end
  end
endmodule
