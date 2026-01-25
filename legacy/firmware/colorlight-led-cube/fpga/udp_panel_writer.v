module udp_panel_writer
                      #(parameter PORT_MSB = 16'h66)
                       (input  wire          clock,
                        input  wire          reset,
                        input  wire          button,
                        input  wire          debug_ip_rx_valid,
                        input  wire          debug_udp_rx_valid,
                        input  wire          udp_source_valid,
                        input  wire          udp_source_last,
                        output reg           udp_source_ready,
                        input  wire  [15:0]  udp_source_src_port,
                        input  wire  [15:0]  udp_source_dst_port,
                        input  wire  [31:0]  udp_source_ip_address,
                        input  wire  [15:0]  udp_source_length,
                        input  wire  [31:0]  udp_source_data,
                        input  wire  [3:0]   udp_source_error,

                        output reg [5:0]     ctrl_en,
                        output wire [3:0]    ctrl_wr,
                        output reg [15:0]    ctrl_addr,
                        output reg [23:0]    ctrl_wdat,

                        output reg led_reg
);

    assign ctrl_wr = 4'b0111;
    assign udp_source_ready = 1'b1;

    reg [26:0] counter = 27'b0;

    wire _unused = &{debug_ip_rx_valid, debug_udp_rx_valid,
                     udp_source_valid, udp_source_last,
                     udp_source_src_port, udp_source_dst_port,
                     udp_source_ip_address, udp_source_length,
                     udp_source_data, udp_source_error, button, reset};

    always @(posedge clock) begin
        counter <= counter + 1;

        // LED shows color state
        led_reg <= counter[26];

        // Fill panel
        ctrl_en <= 6'b000001;
        ctrl_addr <= counter[12:0];

        // Toggle color based on bit 26 (~0.5s at 125MHz)
        if (counter[26])
            ctrl_wdat <= 24'h0000FF;  // BLUE
        else
            ctrl_wdat <= 24'h00FF00;  // GREEN
    end

endmodule
