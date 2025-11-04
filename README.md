GPS Freak - A GPS Disciplined Frequency Generator
=================================================

GPS Freak uses a U-Blox MAX-10 GPS receiver and a Texas Instruments LMK05318b
clock generator to drive multiple clock outputs.  The outputs are a total of 5
SMA connectors, with 4 separate frequencies derived from 2 PLLs.

Power and data are supplied via a USB-C connector (USB FS data rate).  The GPS
unit is available over USB as a CDC ACM serial port.  In addition, there are USB
end-points for device control via the CPU, and python scripts to drive this.

License
=======

The hardware designs in this project are all licensed under CERN Open Hardware
Licence, strongly reciprocal, CERN-OHL-S.  ([cern\_oh\l_s\_v2.txt]).

All software in this project is licensed under the GNU GPL v3.  ([COPYING.txt])

Connectors
==========

The GPS input is a SMA connector that also provides 3.3V (or 5V) antennae power.
The voltage selection is a strap resistor on board.

Outputs 1+ and 1-
: Complementary high frequency, 3MHz to 1.6MHz, 11dbm nominal output power.
  These can be used together as a differential pair.  AC coupled.  If only one
  of the two outputs is used in a noise-sensitive environment, consider
  terminating the other.

Output 2
: High frequency.  3MHz to 1.6GHz, 14dbm nominal output power.  AC coupled.

Output 3
: Low frequency 3.3V CMOS.  Sub-1Hz to 325MHz.  DC coupled.

Output 4
: Mid frequency 3.3V CMOS.  3MHz to 325Mhz.  DC coupled.

USB C.  Power (approx 2W) and data (USB FS).

The high frequency outputs provide complete coverage of 3MHz to 800MHz with
≈nano-Hertz resolution, and then select frequencies to 1.6GHz.  (There is a
region around 520MHz where the LMK05318b is operated outside its documented
range, but I have seen no issue with this.)

The outputs are all nominally 50Ω.  In fact, the LMK05318b appears to have
current-source outputs, so the drive impedance of the high-frequency outputs is
high.  The two CMOS level outputs are each driven by a TLV3601 comparator, from
the datasheet this appears to be a reasonable match for 50Ω.

There are also pads for 3 internal U.Fl connectors.  One provides an additional
output, one a secondary reference clock input to the LMK05318b, and the last an
output of the GPS timepulse signal.  A possible use-case is to divert the GPS
timepulse to an external Rubidium clock, and then use that to drive the
secondary reference.

GPS
===

This is a U-Blox MAX-F10S.  (Building the board with other U-Blox MAX or
compatible devices is possible.  The firmware should work with any U-Blox MAX-10
or later device.)

Status LED
==========

There is a status LED next to the USB connector.  This is a three colour RGB
LED.  The LED lights green or red to indicate that the device has PLL lock.
Additionally, the blue component indicates USB activity.

Housing
=======

The board outline is just under 80mm × 50mm.  A matching extruded aluminium
housing (80mm × 50mm x 20mm) can be readily obtained from websites such as
aliexpress.

Two small kicad projects are in the git repo for end-plates (end-in.kicad\_pro
and end-out.kicad\_pro), with appropriately sized holes for the connectors and
LED.  The screw holes can be counter-sunk with a drill as appropriate.

I recommend checking the board for fit before soldering the SMA connectors.  If
necessary, the edges of the board can be sanded down slightly with sandpaper.

Heat-sinking
------------

The LMK05318b generates a bit of heat, and the board may reach temperatures
approaching 30°C above ambient.  This should not be a problem, but provision
is made to heat-sink the rear of the PCB to the case.

There is a 7mm × 7mm exposed copper area underneath the LMK05318b.  Solder a 2mm
thick copper slug to this, and then cover with a 1.5mm thick thermal pad.  The
clearance between PCB and housing is 3mm, so inserting the board into the
housing will apply gentle pressure to the thermal pad.  Chamfer the appropriate
edge of the housing with a file to make inserting the assembly easier.

(I found 2mm × 6mm × 100mm copper bars on aliexpress, from which a 6mm square
slug can be cut with a hacksaw.  TODO - 10mm wide, rather than 6mm, are easier
to find, so increasing the size of the exposed copper would be useful.)

Software
========

The device software is a mix of on-board firmware, and Python scripts
(py/freak.py) to communicate with the device.  These should work on any OS that
pyusb supports.

The firmware is pretty dumb.  All the frequency planning logic is in the python
scripts, with the firmware essentially just passing through commands to the GPS
and clock generator.  Frequency plans may be saved to flash for autonomous
operation with no USB.

Firmware updates are done via USB DFU.  There is also a Skedd connector breaking
out the CPU SWD with a standard six pin connection.  The CPU boot pin is taken
to a 0.1" header, which can be used to access DFU in case of corrupted firmware.

The python scripts should work with python 3.10 or later.

Control Lines
=============

The CPU has various control lines to the GPS and clock chip.  There is UART to
the GPS, and all are on an I2C bus.  In addition there are connections to:

* EXTINT and RESET lines on GPS.
* PDN, STATUS0, STATUS1/FDEC and GPIO2/FINC on the clock chip.  The latter two
  allow some degree of frequency modulation to be acheived e.g., a basic low
  bit-rate FSK.

Hardware Options
================

The three U.Fl connectors can be soldered.  The GPS timepulse output also
requires soldering a 50Ω 0603 or similar termination component.

The output 2 balun can be removed and replaced with resistors.  Populate the
series resistor R32 with a 0Ω short, and optionally terminate the
second line of the internal differential pair with 50Ω.

Component Substitutions
-----------------------

Some notes on possible component substitutions / changes.

**GPS** Any UBlox MAX-10 is a drop-in replacement.  The software should be
compatible with any UBlox unit with the newer configuration messages (F9 and
later).  Hardware-wise, any U-Blox 18 pin module should be a drop-in
replacement.

**LMK05318b** Do not use the non-b LMK05318:
  * With the 3.3V swing on the reference input, you need to DC couple
    it, which is only supported on the `b' variant.
  * The software uses the programmable PLL2 denominator.

**C0G/NP0 caps** These are probably overkill for the loop-filter caps.  Feel
free to replace with (smaller!) X5R/X7R.  In that case I suggest use parts with
a voltage rating of at least 10V, to avoid the X5R/X7R voltage dependance.

**Ferrite Beads** I used 100Ω @ 100Mhz, because JLCPCB had them as basic parts
(no loading fee).  The TI eval board has 220Ω @ 100MHz.  The choice is probably
not critical, but if you use parts with DC resistance above 0.1Ω then double
check power consumption and voltage drop on the various voltage rails.

I'm not sure how much difference the ferrite beads make in practice.  If you
remove them entirely, then all the point-of-load 10µF caps can be replaced by
0.1µF caps.

**Filter caps** I've used 0402 10µF caps, where-as the LMK05318b datasheet
suggests 10µF 0402 plus 0.1µF 0201.  The main reason is that they are cheaper
with JLCPCB (0201 caps incur loading and assembly fees).

**CPU** Basically any QFN-32 STM32 with USB should be a drop-in replacment
hardware-wise, and require few software modifications—likely only the CPU
initialization and USB clocking will need to change.  Likewise, a CPU in a
different package should require only minor board modifications.

**Voltage regulators** I provisioned 1.8V as well as 3.3V in order to reduce
power dissipation.  TI suggests using LDOs to reduce noise.  Beyond that, the
power supply circuitry is fairly arbitrary.

If you switch to a CPU with that supports 1.8V I/O then you could run the GPS at
1.8V also (but in that case, you'll need to supply 3.3V to the GPS antennae
bias.)  TI suggests using LDOs to power the LMK05318b, but it would work with
just switching regulators.

The feedback divider resistors are chosen to avoid JLCPCB loading fees.
Additionally, fine grained margining of the LDO output voltages can be achieved
by replacing the two 0603 resistors, R56 and R66.

**Programming Header** This can be removed, or swapped for whatever you fancy.
You need to keep one of either the SWD header, or else the boot-pin header, in
order to load firmware.  (Once you have functioning firmware installed, you
can upgrade purely via software command.)

**Temperature Sensor** Populating this is optional.

Hardware Versioning
===================

[TODO - change pin.] CPU pin 13 A7 is reserved for hardware versioning.  To
increment the hardware revision, drive with a resistor divider, approx. 10k
Thevenin resistance or less, to N / 25 × VDD, where N is the revision number.
This is revision 0, so the pin is grounded.

Cost Reduction Opportunities
============================

Really understand the difference between MAX-F10 and MAX-M10.  Does L5
band help much, or does SBAS make this redundant?

The TCXO can be replaced by a cheaper one.  The phase noise to worry about is
capped above at around 18kHz by the BAW oscillator, and at low frequencies by
the GPS output.  We possibly don't even need a TCXO?

The CPU is overkill.  With the current arrangement of dumb firmware and all the
smarts in the Python scripts, a low end CPU would be just fine.

The temperature sensor is only for development.  Once we know how much heat the
board generates, we can drop it.  Or just use a 1¢ thermistor.

Evaluate whether or the C0G capacitors on the loop filter are worthwhile.  I
suspect not in realistic conditions.  The 0.47µF C0G cap could also be replaced
by a through-hole film cap.

Do we need the super cap?  It's only to speed up GPS start-up, but we are likely
to be always on anyway.

I'm not sure the output balun on output 2 is worth the cost.
