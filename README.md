GPS Freak — A GPS Disciplined Frequency Generator
=================================================

GPS Freak uses a U-Blox MAX-10 GPS receiver and a Texas Instruments LMK05318b
clock generator to drive multiple clock outputs.  The outputs are a total of 5
SMA connectors, with 4 separately programmable frequencies derived from 2 PLLs.

Power and data are supplied via a USB-C connector (USB FS data rate).  The GPS
unit is available over USB as a CDC ACM serial port.  In addition, there are USB
end-points for device control via the CPU, and Python scripts to drive this.

License
=======

The hardware designs in this project are all licensed under CERN Open Hardware
Licence, strongly reciprocal, CERN-OHL-S.  (`cern_ohl_s_v2.txt`).

All software in this project is licensed under the GNU GPL v3.  (`COPYING.txt`)

Connectors
==========

**GPS Input** for an active L1 and/or L5 GPS antenna.  It provides 3.3 V (or
5 V) antennae power.  The voltage selection is a strap resistor on board.

**Outputs 1+** and **1–**
: Complementary high frequency, 3 MHz to 1.6 MHz, 11 dbm nominal output power.
  These can be used together as a differential pair.  AC coupled.  If only one
  of the two outputs is used in a noise-sensitive environment, consider
  terminating the other.

**Output 2**
: High frequency.  3 MHz to 1.6 GHz, 14 dbm nominal output power.  AC coupled.

**Output 3**
: Low frequency 3.3 V CMOS.  Sub-1 Hz to 325 MHz.  DC coupled.

**Output 4**
: Mid frequency 3.3 V CMOS.  3 MHz to 325 Mhz.  DC coupled.

**USB-C**
: Power (approx 2 W) and data (USB FS).

The high frequency outputs provide complete coverage of 3 MHz to 800 MHz with ≈
nano-Hertz resolution, and then select frequencies to 1.6 GHz.  (There is a
region around 520 MHz where the LMK05318b is operated outside its documented
range, but I have seen no issue with this.)

The outputs are all nominally 50 Ω.  In fact, the LMK05318b appears to have
current-source outputs, so the drive impedance of the high-frequency outputs is
high.  The two CMOS level outputs are each driven by a TLV3601 comparator, from
the datasheet this appears to be a reasonable match for 50 Ω.

There are also pads for 3 internal U.Fl connectors.  One provides an additional
output, one a secondary reference clock input to the LMK05318b, and the last an
output of the GPS timepulse signal.  A possible use is to divert the GPS
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

The board outline is just under 80 mm × 50 mm.  A matching extruded aluminium
housing (80 mm × 50 mm x 20 mm) can be readily obtained from websites such as
AliExpress.

There are small KiCad projects for end-plates (`end-in.kicad_pro` and
`end-out.kicad_pro`), with appropriately sized holes for the connectors and LED.
The screw holes can be counter-sunk with a drill as appropriate.

I recommend checking the board for fit before soldering the components on the
edges (USB and SMA connectors, and the LED).  If necessary, the edges of the
board can be sanded down slightly with sandpaper.

Heat-sinking
------------

The LMK05318b generates a bit of heat, and the board may reach temperatures
approaching 30 °C above ambient.  This should not be a problem, but provision
is made to heat-sink the rear of the PCB to the case.

There is a 7 mm × 7 mm exposed copper area underneath the LMK05318b.  Solder a
2 mm thick copper slug to this, and then cover with a 1.5 mm thick thermal pad.
The clearance between PCB and housing is 3 mm, so inserting the board into the
housing will apply gentle pressure to the thermal pad.  Chamfer the appropriate
edge of the housing with a file to make inserting the assembly easier.

(I found 2 mm × 6 mm × 100 mm copper bars on AliExpress, from which a 6 mm
square slug can be cut with a hacksaw.  TODO - 10 mm wide, rather than 6 mm, are
easier to find, so increasing the size of the exposed copper would be useful.)

Software
========

The device software is a mix of on-board firmware, and Python scripts
(`py/freak.py`) to communicate with the device.  These should work on any OS that
pyusb supports.

The firmware is pretty dumb.  All the frequency planning logic is in the Python
scripts, with the firmware essentially just passing through commands to the GPS
and clock generator.  Frequency plans may be saved to flash for autonomous
operation with no USB.

Firmware updates are done via USB DFU.  There is also a Skedd connector breaking
out the CPU SWD with a standard six pin connection.  The CPU boot pin is taken
to a 0.1" header, which can be used to access DFU in case of corrupted firmware.

The Python scripts should work with Python 3.10 or later.

Board Manufacture
=================

The schematic has JLCPCB part numbers for most components.  Components that I've
hand assembled are marked with `exclude from BOM´ and may or may not have the
part number.  Review and change these to suit.

The Makefile should generate Gerber and component files, just run `make`.

The I/O traces (GPS, clock outputs, and USB) are sized for appropriate impedance
on the JLCPCB 4-layer default stack-up (7628).  None of them should be
especially critical, but if you use a different stack-up, then it is worth
double-checking and adjusting them.

Components that I have hand-soldered in preference to using JLCPCB assembly:

* JLCPCB doesn't stock the MAX-F10S.  It is fairly easy to hand-solder.  They do
  stock the cheaper MAX-M10S.
* The USB-C connector is not too hard to hand solder, which saves a handling
  fee.
* The SMA connectors.  Position them carefully as they need to align with the
  end-plate holes!  These are edge-launch connectors for a 1.6 mm PCB.  You
  can get these cheaply in bulk from AliExpress.
* The 0402 antennae bias inductor I have hand soldered.  The pads are designed
  for easy hand soldering (one is bigger than the other, solder the smaller pad
  first).
* The three C0G/NPO loop-filter caps (C78, C79, C92) I have hand-soldered as
  JLCPCB doesn't have the exact parts.
* The LED I have hand soldered.  It protrudes past the board edge, I am not
  sure if that poses an issue for PCB manufacturers.
* The TVS protection diodes I have never populated.  The 0402 pads are design
  for hand-soldering just like the inductor above.

If you want to keep assembly costs down, then hand soldering many of the ICs
is an option.  This is especially true if you only want one board, as most
manufacturers insist on making multiple!

There is enough space between most of the ICs and surrounding passive components
to get a soldering iron in, but if you are unsure of your soldering skills and
equipment, then review the board layout before committing yourself to that.

Initial Software Install and Configuration
==========================================

This is a rather manual process at the moment.

Firmware build & install
------------------------

For the firmware build, install a recent \`nightly´ rust compiler, along with
cargo, along with the \`thumbv8m.main-none-eabihf´ target to support the CPU.

Build the firmware with `cargo b`.  This is an alias to build with the correct
compiler options.  The .elf file is left in `target/thumbv8m.main-none-eabihf/release/freak.elf`.

The initial firmware install can either be done via a SWD dongle (J-Link or
similar) or via DFU USB.  Most SWD programmers support `.elf` files directly.
`dfu-utils` is an unfriendly command line tool, the `dfu.sh` script works for
me.  Note that the shell script assumes you have only one DFU capable device
attached.

The `freak` tool is written in Python.  You need Python 3.10 or later.  The only
additional Python module you need should be `pyusb`, which can be installed
using `pip`.

GPS configuration
-----------------

Configuring the GPS can be done either using the `freak` tool, or using the
U-Blox u-center2 software.  Note that setting the host-side UART baud rate
**must** be done using `freak gps baud`, the normal OS baud rate setting will
not work.  I use a baud rate of 230400; the default 9600 is awfully slow.

The timepulse output from the GPS unit defaults to 1 Hz.  This needs to be
increased. I use 8844582 Hz, and `freak` by default assumes that this is the
case.  The GPS unit has separate settings for GPS locked and unlocked, I
recommend only set the GPS locked frequency, as the firmware currently does not
actively monitor for GPS lock.  You need to set a few things:
  * `CFG-TP-FREQ_LOCK_TP1=8844582` — the frequency
  * `CFG-TP-PULSE_LENGTH_DEF=0` — pulse length is duty cycle
  * `CFG-TP-DUTY_LOCK_TP1=50.0` — duty cycle.

My complete list of config changes on the MAX-F10S:

|Config Item|Value|
|-----------|-----|
|CFG-TP-PULSE_DEF|1|
|CFG-TP-FREQ_LOCK_TP1|8844582|
|CFG-TP-DUTY_LOCK_TP1|50.0|
|CFG-TP-PULSE_LENGTH_DEF|0x00|
|CFG-RATE-MEAS|100|
|CFG-RATE-NAV|10|
|CFG-SBAS-USE_TESTMODE|True|
|CFG-SBAS-PRNSCANMASK|0|
|CFG-UART1-BAUDRATE|230400|
|CFG-MSGOUT-NMEA_ID_GSV_UART1|1|

One you are happy with the GPS config, save it to flash with `freak gps save`.

Note that it is important that you **don't** use a round number (like 10 MHz)
for the time-pulse frequency!  This is because the time-pulse is generated by
applying a fractional divider to a nominally, but not exactly, 64 MHz crystal.
Hence round numbers tend to end up with awful low-frequency aliasing spurs.

LMK05318b Configuration
-----------------------

First upload a configuration generated by the TI TICS/Pro tool: `freak clock
upload py/freak/lowbw.tcs`.  This contains many settings, such as the PLL loop
filters, that are calculated via unknown formulæ or simply undocumented.

After that, you can use `freak` to set specific frequencies, e.g., `freak freq
10 10 1Hz` to give two 10 MHz and a 1 Hz output.

Once you are happy, save to flash with `freak clock save`.

Control Lines
=============

The CPU has various control lines to the GPS and clock chip.  There is UART to
the GPS, and all are on an I2C bus.  In addition, there are connections to:

* EXTINT and RESET lines on GPS.
* PDN, STATUS0, STATUS1 / FDEC and GPIO2 / FINC on the clock chip.  The latter
  two allow some degree of frequency modulation to be achieved e.g., a basic low
  bit-rate FSK.

Hardware Options
================

The three U.Fl connectors can be soldered.  The GPS timepulse output also
requires soldering a 50 Ω 0603 or similar termination component.

The output 2 balun can be removed and replaced with resistors.  Populate the
series resistor R32 with a 0 Ω short, and optionally terminate the second line
of the internal differential pair with 50 Ω.

Component Substitutions
-----------------------

Some notes on possible component substitutions / changes.

**GPS** Any U-Blox MAX-10 is a drop-in replacement.  The software should be
compatible with any U-Blox unit with the newer configuration messages (F9 and
later).  Hardware-wise, any U-Blox 18-pin module should be electrically
compatible.

**LMK05318b** Do not use the non-`b´ LMK05318:
  * With the 3.3 V swing on the reference input, you need to DC couple it, which
    is only supported on the `b´ variant.
  * The software uses the programmable PLL2 denominator.

**C0G/NP0 caps** These are probably overkill for the loop-filter caps.  Feel
free to replace with (smaller and cheaper) X5R / X7R.  In that case I suggest
use parts with a voltage rating of at least 10V, to minimize the X5R / X7R
voltage dependence.  C78 is only in the schematic because the physically larger
C79 is some distance from the LMK05318b.  If you use smaller components, then
change C78 to 470 nF and remove C79.

**Ferrite Beads** I used 100 Ω @ 100 MHz, because JLCPCB had them as basic parts
(no loading fee).  The TI evaluation board has 220 Ω @ 100 MHz.  The choice is
probably not critical, but if you use parts with DC resistance above 0.1 Ω then
double check power consumption and voltage drop on the various voltage rails.

I'm not sure how much difference the ferrite beads make in practice.  If you
remove them entirely, then all the point-of-load 10 µF caps on the power rails
can be replaced by 0.1µF caps.  (Don't change C77, C91 and C93 though.)

**Filter caps** I've used 0402 10 µF caps, where-as the LMK05318b datasheet
suggests 10 µF 0402 plus 0.1 µF 0201.  The main reason is that they are cheaper
with JLCPCB (0201 caps incur loading and assembly fees).

**CPU** Basically any QFN-32 STM32 with USB should be a drop-in replacement
hardware-wise, and require few software modifications—likely only the CPU
initialization and USB clocking will need to change.  Likewise, a CPU in a
different package should require only minor board modifications.

**Voltage regulators** I provisioned 1.8 V as well as 3.3 V in order to reduce
power dissipation.  TI suggests using LDOs to reduce noise.  Having the two
switching regulators before the LDOs reduces power consumption and heating,
without them the LDO would probably overheat.  Beyond that, the power supply
circuitry is fairly arbitrary.

If you switch to a CPU with that supports 1.8 V I/O then you could run the GPS
at 1.8 V also (but in that case, you'll need to supply 3.3 V to the GPS antennae
bias.)

The feedback divider resistors are chosen to avoid JLCPCB loading fees.
Additionally, fine-grained margining of the LDO output voltages can be achieved
by replacing the two 0603 resistors, R56 and R66.

**Programming Header** This can be removed, or swapped for whatever you fancy.
You need to keep one of either the SWD header, or else the boot-pin header, in
order to load firmware.  (Once you have functioning firmware installed, you can
upgrade purely via software command.)

**Temperature Sensor** Populating this is optional.

**LED** This is a common 1206 edge-mount footprint RGB LED.  If you use a
different part, then double or triple check the footprint, and note the odd pad
numbering.  The resistor on the green line is a higher value, as the green
component seems to be much brighter than the red and blue.  That resistor is an
0603 footprint to make it easier to swap if desired.

Hardware Versioning
===================

[TODO - change pin.] CPU pin 13 A7 is reserved for hardware versioning.  The
plan is to use the CPU ADC to read a resistor voltage divider.  To change the
hardware revision, drive with a resistor divider, approx. 10k Thevenin
resistance or less, to N / 25 × VDD, where N is the revision number.  This is
revision 0, so the pin is grounded.

Cost Reduction Opportunities
============================

Really understand the difference between MAX-F10 and MAX-M10.  Does L5 band help
much, or does SBAS make this redundant?

The TCXO can be replaced by a cheaper one.  The phase noise to worry about is
capped above at around 18 kHz by the BAW oscillator, and at low frequencies by
the GPS output.  We possibly don't even need a TCXO?

The CPU is overkill.  With the current arrangement of dumb firmware and all the
smarts in the Python scripts, a low-end CPU would be just fine.

The temperature sensor is only for development.  Once we know how much heat the
board generates, we can drop it.  Or just use a 1 ¢ thermistor.

Evaluate whether or the C0G capacitors on the loop filter are worthwhile.  I
suspect not in realistic conditions.  The 0.47 µF C0G cap could also be replaced
by a through-hole film cap.

Do we need the super cap?  It's only to speed up GPS start-up, but we are likely
to be always on anyway.

I'm not sure the output balun on output 2 is worth the cost.

More on the `freak` tool
========================

The `freak` tool can perform various operations as well as setting the output
frequencies.  Everything has a `--help` option which is hopefully helpful.

`freak freq` with no arguments displays the current output frequencies.

`freak drive` can display and set the various output drive strengths.

`freak plan` can carry out the frequency planning computations without writing
them to the device.  This is useful to explore what is possible.

`freak clock get/set` and get and set individual clock generater registers.  The
TI documentation lists most but not all of these.  The file
`py/freak/lmk05318b.list` has a few additions that I've gleaned from other
sources.

`freak gps get/set` likewise get and set GPS config registers.

For more advanced programming of the LMK05318b, your best option is to generate
a .tcs file with the TI TICS/Pro tool, and write to the device with `freak clock
upload`.

`freak info`, `freak clock status`, `freak gps info` and `freak gps status` all
output various bits of hardware information and status, in varying degrees of
understandability.

`freak gps changes` shows the difference between the current GPS configuration
and the factory defaults.

`freak clock dump` or `freak gps dump` print the entire clock generator or GPS
configuration, respectively.

Known Issues
============

Currently for some frequency plans, the frequency-lock loss detection is not
configured correctly.  If the LED status red, and `freak clock status` shows
that LOPL\_DPLL is good but LOFL\_PLL is bad, then run `freak clock set
LOFL_DPLL_MASK=1`; `freak clock status` to mask the problematic flag.

If frequency planning fails, then `freak` tends to die with an unfriendly
assertion failure rather than a useful error message.  Ditto various other error
conditions.  Problems with the communication to the GPS unit likely result in
hangs.
