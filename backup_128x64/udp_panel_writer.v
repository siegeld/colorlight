module udp_panel_writer
                      #(parameter PORT_MSB = 16'h17)  // Port 6000 = 0x1770, upper byte = 0x17
                       (input  wire          clock,
                        input  wire          reset,
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

    assign ctrl_wr = 4'b0111; // always write RGB

    localparam STATE_WAIT_PACKET = 2'b01, STATE_READ_DATA = 2'b10;

    // Debug: count udp_source_valid pulses
    reg [23:0] valid_counter = 0;
    always @(posedge clock) begin
        if (udp_source_valid)
            valid_counter <= valid_counter + 1;
    end

    reg [5:0] ctrl_en_reg;
    reg [1:0] udp_state;
    reg [15:0] source_port;
    reg [15:0] dest_port;
    reg [31:0] src_ip;
    reg [31:0] data;
    reg [1:0] byte_count;
    initial udp_source_ready <= 1'b0;

    always @(posedge clock) begin
        led_reg <= ~valid_counter[4];  // Toggle LED every 16 udp_source_valid pulses
        if (reset) begin
            udp_source_ready <= 1'b0;
            udp_state <= STATE_WAIT_PACKET;
            ctrl_en_reg <= 6'b0;
            ctrl_addr <= 16'b0;
            ctrl_wdat <= 16'b0;
            ctrl_en   <= 1'b0;
	          data      <= 32'b0;
            byte_count <= 2'b0;
        end else begin
            ctrl_en <= 6'b0;
            case (udp_state)
                STATE_WAIT_PACKET : begin
                    udp_source_ready <= 1'b1;
                    if (udp_source_valid) begin  // Accept all UDP packets (port filtered by liteeth_core)
                        ctrl_en_reg <= 6'b000001;  // Always enable panel 0 (port 6000 = 0x1770)
                        if (!udp_source_last) begin
			                     data = {data[23:0],udp_source_data[7:0]};
			                     byte_count  <= 3'b1;
                           udp_state   <= STATE_READ_DATA;
                        end
                    end
                end
                STATE_READ_DATA : begin
                    if (udp_source_valid) begin
                        byte_count <= byte_count + 3'b1;
			                  data = {data[23:0],udp_source_data[7:0]};
			                  if (byte_count == 3'b11) begin
	                        ctrl_en          <= ctrl_en_reg;
	                        ctrl_addr        <= data[31:18];
	                        ctrl_wdat[23:16] <= data[17:12];
	                        ctrl_wdat[15:8]  <= data[11:6];
	                        ctrl_wdat[7:0]   <= data[5:0];
	                        led_reg          <= ~led_reg;  // Toggle LED on each pixel write
			                  end

                        if (udp_source_last) begin
                            udp_state <= STATE_WAIT_PACKET;
                        end
                    end
                end
            endcase
        end
    end

endmodule
