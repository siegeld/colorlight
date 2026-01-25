// Simple LED blink test for Colorlight 5A-75E V8.x
// Use this to verify JTAG programming works before attempting full firmware
//
// The onboard LED should blink at approximately 1Hz

`default_nettype none

module blink_test (
    input  wire osc25m,     // 25MHz clock from PHY
    output wire led,        // Onboard LED (active low)
    output wire phy_resetn  // Keep PHY out of reset so clock runs
);

    // Keep PHY running so we get the 25MHz clock
    assign phy_resetn = 1'b1;

    // 25-bit counter for ~1Hz blink at 25MHz
    // 2^24 = 16,777,216 -> ~1.5Hz
    // 2^25 = 33,554,432 -> ~0.75Hz
    reg [24:0] counter = 0;

    always @(posedge osc25m) begin
        counter <= counter + 1;
    end

    // LED is active low
    assign led = ~counter[24];

endmodule
