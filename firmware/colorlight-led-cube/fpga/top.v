`default_nettype none
module top
    (
    input wire osc25m,
    /*
     * RGMII interface
     */
    input  wire                       rgmii_rx_clk,
    input  wire [3:0]                 rgmii_rxd,
    input  wire                       rgmii_rx_ctl,
    output wire                       rgmii_tx_clk,
    output wire [3:0]                 rgmii_txd,
    output wire                       rgmii_tx_ctl,
    /*
     * MDIO interface
     */
    output wire mdio_scl,
    output wire mdio_sda,
    /*
     * USER I/O (Button, LED)
     */
    input wire button,
    output wire led,
    output wire phy_resetn,

    output wire [5:0] R0,
    output wire [5:0] G0,
    output wire [5:0] B0,
    output wire [5:0] R1,
    output wire [5:0] G1,
    output wire [5:0] B1,
    output wire A,
    output wire B,
    output wire C,
    output wire D,
    output wire E, // for 1/32 scan
    output wire LAT,
    output wire OE, //blank
    output wire CLK

);

    //------------------------------------------------------------------
    // PLL Instantiation and Locked Reset generation
    //------------------------------------------------------------------

    wire phy_init_done;
    wire                 locked;
    wire                 clock;
    reg [3:0]            locked_reset = 4'b1111;
    wire                 reset = locked_reset[3];
    wire                 display_clock;

    pll pll_inst(.clkin(osc25m),.clock(clock),.panel_clock(display_clock),.locked(locked));

    always @(posedge clock or negedge locked) begin
        if (locked == 1'b0) begin
            locked_reset <= 4'b1111;
        end else begin
            locked_reset <= {locked_reset[2:0], 1'b0};
        end
    end

    wire          udp_sink_valid       = 1'b0;
    wire          udp_sink_last        = 1'b0;
    wire          udp_sink_ready       ;
    wire  [15:0]  udp_sink_src_port    = 16'b0;
    wire  [15:0]  udp_sink_dst_port    = 16'b0;
    wire  [31:0]  udp_sink_ip_address  = 32'b0;
    wire  [15:0]  udp_sink_length      = 16'b0;
    wire  [31:0]  udp_sink_data        = 32'b0;
    wire  [3:0]   udp_sink_error       = 4'b0;
    wire          udp_source_valid     ;
    wire          udp_source_last      ;
    wire          udp_source_ready     ;
    wire  [15:0]  udp_source_src_port  ;
    wire  [15:0]  udp_source_dst_port  ;
    wire  [31:0]  udp_source_ip_address;
    wire  [15:0]  udp_source_length    ;
    wire  [31:0]  udp_source_data      ;
    wire  [3:0]   udp_source_error     ;

    phy_sequencer phy_sequencer_inst (.clock(clock),
                  .reset(reset),
                  .phy_resetn(phy_resetn),
                  .mdio_scl(mdio_scl),
                  .mdio_sda(mdio_sda),
                  .phy_init_done(phy_init_done));

    liteeth_core eternit (
        /* input         */ .sys_clock            (clock                ),
        /* input         */ .sys_reset            (reset & ~phy_init_done),
        /* output        */ .rgmii_eth_clocks_tx  (rgmii_tx_clk         ),
        /* input         */ .rgmii_eth_clocks_rx  (rgmii_rx_clk         ),
        /* output        */ .rgmii_eth_rst_n      (                     ),
        /* input         */ .rgmii_eth_int_n      (                     ),
        /* inout         */ .rgmii_eth_mdio       (                     ),
        /* output        */ .rgmii_eth_mdc        (                     ),
        /* input         */ .rgmii_eth_rx_ctl     (rgmii_rx_ctl         ),
        /* input  [3:0]  */ .rgmii_eth_rx_data    (rgmii_rxd            ),
        /* output        */ .rgmii_eth_tx_ctl     (rgmii_tx_ctl         ),
        /* output [3:0]  */ .rgmii_eth_tx_data    (rgmii_txd            ),
        /* input         */ .udp_sink_valid       (udp_sink_valid       ),
        /* input         */ .udp_sink_last        (udp_sink_last        ),
        /* output        */ .udp_sink_ready       (udp_sink_ready       ),
        /* input [15:0]  */ .udp_sink_src_port    (udp_sink_src_port    ),
        /* input [15:0]  */ .udp_sink_dst_port    (udp_sink_dst_port    ),
        /* input [31:0]  */ .udp_sink_ip_address  (udp_sink_ip_address  ),
        /* input [15:0]  */ .udp_sink_length      (udp_sink_length      ),
        /* input [31:0]  */ .udp_sink_data        (udp_sink_data        ),
        /* input [3:0]   */ .udp_sink_error       (udp_sink_error       ),
        /* output        */ .udp_source_valid     (udp_source_valid     ),
        /* output        */ .udp_source_last      (udp_source_last      ),
        /* input         */ .udp_source_ready     (udp_source_ready     ),
        /* output [15:0] */ .udp_source_src_port  (udp_source_src_port  ),
        /* output [15:0] */ .udp_source_dst_port  (udp_source_dst_port  ),
        /* output [31:0] */ .udp_source_ip_address(udp_source_ip_address),
        /* output [15:0] */ .udp_source_length    (udp_source_length    ),
        /* output [31:0] */ .udp_source_data      (udp_source_data      ),
        /* output [3:0]  */ .udp_source_error     (udp_source_error     )
    );

    wire [5:0]  ctrl_en;
    wire [3:0]  ctrl_wr;
    wire [15:0] ctrl_addr;
    wire [23:0] ctrl_wdat;

    udp_panel_writer udp_inst
                    (.clock(clock),
                     .reset(reset),

                     .udp_source_valid(udp_source_valid),
                     .udp_source_last(udp_source_last),
                     .udp_source_ready(udp_source_ready),
                     .udp_source_src_port(udp_source_src_port),
                     .udp_source_dst_port(udp_source_dst_port),
                     .udp_source_ip_address(udp_source_ip_address),
                     .udp_source_length(udp_source_length),
                     .udp_source_data(udp_source_data),
                     .udp_source_error(udp_source_error),

                     .ctrl_en(ctrl_en),
                     .ctrl_wr(ctrl_wr),
                     .ctrl_addr(ctrl_addr),
                     .ctrl_wdat(ctrl_wdat),
                     .led_reg(led)
                     );

    // Reduced to 1 panel to fit on ECP5-25K
    // Original design had 6 panels but that exceeds FPGA capacity

    wire A_int;
    wire B_int;
    wire C_int;
    wire D_int;
    wire E_int;
    wire LAT_int;
    wire OE_int;
    wire CLK_int;

    ledpanel panel_inst (
        .ctrl_clk(clock),
        .ctrl_en(ctrl_en[0]),
        .ctrl_wr(ctrl_wr),       // Which color memory block to write
        .ctrl_addr(ctrl_addr),   // Addr to write color info on [col_info][row_info]
        .ctrl_wdat(ctrl_wdat),   // Data to be written [R][G][B]

        .display_clock(display_clock),
        .panel_r0(R0[0]),
        .panel_g0(G0[0]),
        .panel_b0(B0[0]),
        .panel_r1(R1[0]),
        .panel_g1(G1[0]),
        .panel_b1(B1[0]),
        .panel_a(A_int),
        .panel_b(B_int),
        .panel_c(C_int),
        .panel_d(D_int),
        .panel_e(E_int),
        .panel_clk(CLK_int),
        .panel_stb(LAT_int),
        .panel_oe(OE_int)
    );

    // Directly connect panel outputs
    assign R0[5:1] = 5'b0;
    assign G0[5:1] = 5'b0;
    assign B0[5:1] = 5'b0;
    assign R1[5:1] = 5'b0;
    assign G1[5:1] = 5'b0;
    assign B1[5:1] = 5'b0;

    assign A = A_int;
    assign B = B_int;
    assign C = C_int;
    assign D = D_int;
    assign E = E_int;
    assign LAT = LAT_int;
    assign OE  = OE_int;
    assign CLK = CLK_int;
endmodule
