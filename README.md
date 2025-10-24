Yet another GPS Disciplined Oscillator
======================================

GPS Feak uses a U-Blox MAX-10 GPS receiver and a Texas Instruments LMK05318(B)
clock generator to drive multiple clock outputs.  The outputs are a total of 5
SMA connectors.

Power and data is supplied via a USB-C connector (USB FS data rate).  The
GPS unit is available over USB as a CDC ACM serial port.

License
=======

The hardware designs in this project are all licensed under CERN Open Hardware
Licence, strongly reciprocal, CERN-OHL-S.  ([cern\_oh\l_s\_v2.txt]).

All software in this project is licensed under the GNU GPL v3.  ([COPYING.txt])


Connectors
==========

GPS input, SMA, provides 3.3V (or 5V) antennae power.  The voltage selection
is a strap resistor on board.

Outputs 1+ and 1-
: Complementary high frequency, 3MHz to 1.6MHz.  These can be used together as a
  differential pair.  AC coupled.  If only one of the two outputs is used in a
  noise-sensitive environment, consider terminating the other.

Output 2
: High frequency, higher power levels.  3MHz to 1.6GHz.  AC coupled.

Output 3
: Low frequency 3.3V CMOS.  Sub-1Hz to 325MHz.  DC coupled.

Output 4
: Mid frequency 3.3V CMOS.  3MHz to 325Mhz.  DC coupled.

USB C.  Power (approx 2W) and data (USB FS).

The outputs are all nominally 50Ω.  In fact, the LMK05318b appears to have
current-source outputs, so the drive impedance of the high-frequency outputs is
high.  The two CMOS level outputs are each driven by a TLV3601 comparator, from
the datasheet this appears to be a reasonable match for 50Ω.

There are also pads for 3 internal U.Fl connectors.  One provides an additional
output, one a secondary reference clock input to the LMK05318, and the last
an output of the GPS timepluse signal.

GPS
===

This is a U-Blox MAX-F10S.  (Building the board with other U-Blox MAX or
compatible devices is possible.  The firmware should work with any U-Blox MAX-10
or later device.

Software
========

The device software is a mix of on-board firmware, and Python scripts (freak.py)
to communicate with the device.  These should work on any OS that pyusb
supports.

Firmware updates are done via USB DFU.

Hardware Options
================

The three U.Fl connectors can be soldered.  The GPS timepulse output also
requires soldering a 50Ω 0603 or similar termination component.

The output 2 balun can be removed and replaced with resistors.  Populate the
series resistor R32 with a 0Ω short, and optionally terminate with 50Ω the
second line of the internal differential pair.

Hardware Versioning
===================

[TODO - change pin.] CPU pin 13 A7 is reserved for hardware versioning.  To
increment the hardware revision, drive with a resistor divider, approx. 10k
Thevenin resistance or less, to N / 25 × VDD, where N is the revision number.
This is revision 0, so the pin is grounded.

Cost Reduction Opportunities
============================

These will simplify the board layout also...

Really understand the difference between MAX-F10 and MAX-M10.  Does L5
band help, or does SBAS make this redundant?

The TCXO can be replaced by a cheaper one.  The phase noise to worry about is
capped above at around 18kHz by the BAW oscillator, and at low frequencies by
the GPS output.  We possibly don't even need a TCXO?

The temperature sensor is only for development.  Once we know how much heat the
board generates, we can drop it.  Or just use a 1¢ thermistor.

Evaluate whether or the C0G caps on the loop filter are worthwhile.  I suspect
not in realistic conditions.

Do we need the super cap?  It's only for hot-start, but we are likely to be
always on anyway.

I'm not sure the output balun on output 2 is worth the cost.
