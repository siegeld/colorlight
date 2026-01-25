// PLL LED blink test for Colorlight 5A-75E V8.x
// Tests if PLL locks correctly with 25MHz input from PHY

`default_nettype none

module pll_blink_test (
    input  wire osc25m,     // 25MHz clock from PHY
    output wire led,        // Onboard LED (active low)
    output wire phy_resetn  // Keep PHY out of reset
);

    // Keep PHY running
    assign phy_resetn = 1'b1;

    // PLL signals
    wire clock_125mhz;
    wire locked;

    // Simple PLL: 25MHz -> 125MHz
    (* FREQUENCY_PIN_CLKI="25" *)
    (* FREQUENCY_PIN_CLKOP="125" *)
    EHXPLLL #(
        .PLLRST_ENA("DISABLED"),
        .INTFB_WAKE("DISABLED"),
        .STDBY_ENABLE("DISABLED"),
        .DPHASE_SOURCE("DISABLED"),
        .OUTDIVIDER_MUXA("DIVA"),
        .CLKI_DIV(1),
        .CLKOP_ENABLE("ENABLED"),
        .CLKOP_DIV(5),
        .CLKOP_CPHASE(2),
        .CLKOP_FPHASE(0),
        .FEEDBK_PATH("CLKOP"),
        .CLKFB_DIV(5)
    ) pll_inst (
        .RST(1'b0),
        .STDBY(1'b0),
        .CLKI(osc25m),
        .CLKOP(clock_125mhz),
        .CLKFB(clock_125mhz),
        .CLKINTFB(),
        .PHASESEL0(1'b0),
        .PHASESEL1(1'b0),
        .PHASEDIR(1'b1),
        .PHASESTEP(1'b1),
        .PHASELOADREG(1'b1),
        .PLLWAKESYNC(1'b0),
        .ENCLKOP(1'b0),
        .LOCK(locked)
    );

    // Counter for ~1Hz blink at 125MHz
    // 2^27 = 134,217,728 -> ~0.93Hz at 125MHz
    reg [26:0] counter = 0;

    always @(posedge clock_125mhz) begin
        if (!locked)
            counter <= 0;
        else
            counter <= counter + 1;
    end

    // LED shows: locked AND blinking
    // If PLL locks: LED blinks
    // If PLL doesn't lock: LED stays off (or on depending on initial state)
    assign led = locked ? ~counter[26] : 1'b1;

endmodule
