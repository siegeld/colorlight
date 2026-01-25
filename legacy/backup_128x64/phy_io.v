// Simple PHY sequencer for RTL8211FD
// RTL8211FD uses hardware strapping for RGMII mode - no MDIO init needed
// IMPORTANT: Do NOT reset the PHY - the 25MHz clock comes from it!
`default_nettype none
module phy_sequencer(input wire clock,
                     input wire reset,
                     output wire phy_resetn,
                     output wire mdio_scl,
                     output wire mdio_sda,
                     output reg phy_init_done);

    // Keep PHY out of reset - clock comes from PHY!
    assign phy_resetn = 1'b1;

    // MDIO idle state
    assign mdio_scl = 1'b1;
    assign mdio_sda = 1'b1;

    // Simple delay then mark init done
    reg [15:0] delay_counter = 0;

    always @(posedge clock) begin
        if (reset) begin
            delay_counter <= 0;
            phy_init_done <= 1'b0;
        end else begin
            if (!phy_init_done) begin
                delay_counter <= delay_counter + 1;
                if (delay_counter == 16'hFFFF) begin
                    phy_init_done <= 1'b1;
                end
            end
        end
    end

endmodule
