
from .serhelper import Serial
from .ublox_msg import UBloxReader
from .usb_endpoint import USBEndpointIO

import freak.message as message
import argparse, io, sys
import usb.core # pyright: ignore

from usb.core import Device as USBDevice # pyright: ignore

class Device:
    args: argparse.Namespace | None

    usb: USBDevice | None = None
    serial: io.IOBase | None = None

    def __init__(self, args: argparse.Namespace|None = None):
        self.args = args

    def get_usb(self) -> USBDevice:
        if self.usb is not None:
            return self.usb

        opts = {}
        if self.args and self.args.name:
            opts['serial_number'] = self.args.name
        gen = usb.core.find(True, manufacturer='Ralph', product='GPS Freak',
                            **opts)
        u = list(gen) # type: ignore
        if self.args and self.args.cpu is not None:
            u = [dev for dev in u if message.get_serial_number(dev)
                 == self.args.cpu]
        if len(u) == 0:
            print('No GPS Freak USB device found', file=sys.stderr)
            sys.exit(1)
        if len(u) > 1:
            print('Multiple GPS Freak USB devices found.',
                  file=sys.stderr)
            print('You may select one with the --sn or --name option.',
                  file=sys.stderr)
            print('Available names and CPU ser. no. are:', file=sys.stderr)
            for d in u:
                cpu = message.get_serial_number(d)
                print(f'    {d.serial_number} {cpu}', file=sys.stderr)
            sys.exit(1)
        assert isinstance(u[0], USBDevice)
        #print(u.serial_number)
        self.usb = u[0]
        # Flush any stale data.
        try:
            self.usb.read(0x83, 64, 10) # pyright: ignore
        except usb.core.USBTimeoutError:
            pass

        # TODO - ping and check.

        return self.usb

    def get_serial(self) -> io.IOBase:
        if self.serial is not None:
            return self.serial

        if self.args is not None and \
           getattr(self.args, 'serial', None) is not None:
            self.serial = Serial(self.args.serial, self.args.baud)
        else:
            self.serial = USBEndpointIO(self.get_usb(), 0, 0x01, 0x81)

        return self.serial

    def get_ublox(self) -> UBloxReader:
        return UBloxReader(self.get_serial())
