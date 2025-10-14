
from .serhelper import Serial
from .ublox_msg import UBloxReader
from .usb_endpoint import USBEndpointIO

import argparse
import io
import usb

from usb.core import Device as USBDevice

class Device:
    args: argparse.Namespace

    usb: USBDevice | None = None
    serial: io.IOBase | None = None

    def __init__(self, args: argparse.Namespace|None = None):
        self.args = args if args is not None else argparse.Namespace()

    def get_usb(self) -> USBDevice:
        if self.usb is not None:
            return self.usb

        # FIXME - arguments for specifying USB device.
        self.usb = usb.core.find(idVendor=0xf055, idProduct=0xd448)
        # Flush any stale data.
        try:
            self.usb.read(0x83, 64, 10)
        except usb.core.USBTimeoutError:
            pass

        # TODO - ping and check.

        return self.usb

    def get_serial(self) -> io.IOBase:
        if self.serial is not None:
            return self.serial

        if self.args.serial is not None:
            self.serial = Serial(self.args.serial)
        else:
            self.serial = USBEndpointIO(self.get_usb(), 0, 0x01, 0x81)

        return self.serial

    def get_ublox(self) -> UBloxReader:
        return UBloxReader(self.get_serial())
