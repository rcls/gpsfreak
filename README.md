Yet another GPS Disciplined Oscillator
======================================

This uses a U-Blox MAX-10 GPS receiver and a Texas Instruments LMK05318(B) clock
generator to drive multiple clock outputs.  The outputs are 4 SMA connectors,
two high-frequency (3.5MHz to 1.2GHz) and two low-frequency (<1Hz to 200MHz).

There is an SMA connector for the GPS antenna input.  Additionally, one of the
low-frequency outputs can be repurposed as a reference clock input, bypassing
GPS.

Power and data is supplied via a USB-C connector (USB FS data rate).


Connectors
==========

GPS input, SMA, provides 3.3V (or 5V) antennae power.  The voltage selection
is a strap resistor on board.

Outputs 1 and 2
: High frequency, highish power levels.  3.5MHz to 1.2GHz+.

Output 3
: Low frequency high drive CMOS (74LVC drive).  sub-1Hz to approx 100MHz.  The
  CMOS output buffer can be bypassed via strap resistors, dramatically lowering
  the output drive but extending the frequency range.

Output 4
: Switchable between output and secondary reference input.  As an output, gives
  CMOS or 50ohm drive, should be good to 500MHz+.  As a secondary reference
  input, the nominal frequency is 10MHz to support failover, but the hardware
  supports operating it down to 1Hz.

  The connector is DC coupled.  As an input the board is built with 300ohm
  termination and a divider to match 3.3V drive to the clock chip.  However
  swapping resistors can change to 50ohm, and can be AC coupled.

Output 5
: Internal Hirose U.Fl connector.  Connects straight to a clock generator output
  pin, do what you wish with it.  Unlike the external outputs, there is no
  protection external to the clock generator IC.

USB C.  Power (approx 2W) and data (USB FS).


GPS
===

This is a U-Blox MAX-F10S.  (Building the board with other U-Blox MAX or
compatible devices is possible).


Hardware Versioning
===================

CPU pin 13 A7 is reserved for hardware versioning.  To increment the hardware
revision, drive with a resistor divider, approx. 10k Thevenin resistance or
less, to N / 25 × VDD, where N is the revision number.  This is revision 0, so
the pin is grounded.

Cost Reduction Opertunities
===========================

These will simplify the board layout also...

Really understand the difference between MAX-F10 and MAX-M10.  Do the L5/L2
bands help, or does SBAS make them redundant?

The power supply network is over-engineered.  We have four output channels (3 ×
3.3V and 1 × 1.8V).  Likely we only need one...

The TCXO can be replaced by a cheaper one.  The phase noise to worry about is
capped above at around 18kHz by the BAW oscillator, and at low frequencies by
the GPS output.  We possibly don't even need a TCXO?

The temperature sensor is only for development.  Once we know how much heat the
board generates, we can drop it.

Evaluate whether or the C0G caps on the loop filter are worthwhile.  I suspect
not in realistic conditions.

Do we need the super cap?  It's only for hot-start, but we are likely to be
always on anyway.

Probably no need for the CPU XTAL, it is there just-in-case.

What drive levels are useful?  Potentially get rid of the baluns & lose some of
the drive power.

The headers are just for development.  Once stuff is working, we just need
to make sure we can force entry into the CPU bootloader.