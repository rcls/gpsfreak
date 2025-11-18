
from fractions import Fraction

# All the frequencies are in MHz.
MHz = Fraction(1)
kHz = MHz / 1000
Hz = kHz / 1000

REF_FREQ = 8844582 * Hz

# I've only seen TICS Pro use this, which means it's the only possibility that I
# have PLL2 filter settings for.  Looking at the documentation, its purpose is
# to reduce the ≈2500MHz BAW frequency down to within the 150MHz PFD frequency
# limit of PLL2—hence the lack of need to change it.
FPD_DIVIDE = 18

# PLL1 frequency range.  ±50ppm
BAW_FREQ = 2_500 * MHz
BAW_LOW  = BAW_FREQ - BAW_FREQ * 50 / 1000000
BAW_HIGH = BAW_FREQ + BAW_FREQ * 50 / 1000000

# We have a 30.72MHz XO.  (This gets doubled at the PLL1 PFD).
XO_FREQ = 30720 * kHz

# This is the official range of the LMK05318b...
OFFICIAL_PLL2_LOW = 5_500 * MHz
OFFICIAL_PLL2_HIGH = 6_250 * MHz

# We push it by 110MHz in each direction, to cover all frequencies up to
# 800MHz
PLL2_LOW = 5340 * MHz
PLL2_HIGH = 6410 * MHz

# Small frequencies...
SMALL = 50 * kHz

PLL2_MID = (PLL2_LOW + PLL2_HIGH) / 2
# Clamp the length of a PLL2 brute force search to ±MAX_HALF_RANGE attempts
# around the mid-point.  This is ±10700 (i.e., 21401 total).
MAX_HALF_RANGE = (PLL2_HIGH - PLL2_LOW) / 2 // SMALL

ZERO = Fraction(0)

# Our numbering of channels:
# 0 = LMK 0,1, GPS Freak 2
# 1 = LMK 2,3, GPS Freak 1
# 2 = LMK 4.
# 3 = LMK 5, GPS Freak U.Fl
# 4 = LMK 6, GPS Freak 4
# 5 = LMK 7, GPS Freak 3, can do 1Hz.
# Index of the output with the stage2 divider.
BIG_DIVIDE = 5
