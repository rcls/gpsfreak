'''Expose a usb endpoint (pair) as a io.RawIOBase object.

We use this as an alternative way to access the serial port, because doing
everything via pyusb makes it trivial to correlate the serial port and command
endpoint—every OS serial driver goes out of its way to make it difficult to
get a handle on the underlying USB device.'''

import io
import usb.core

from collections.abc import ByteString
from typing import Any, cast

class USBEndpointIO(io.IOBase):
    in_buffer: bytearray = bytearray()
    usb_: usb.core.Device
    write_endpoint: int
    read_endpoint: int
    timeout: float
    chunk_size: int

    def __init__(self, device: usb.core.Device,
                 interface: Any, write_endpoint: int, read_endpoint: int,
                 timeout: int = 1000, chunk_size: int = 64):
        self.in_buffer = bytearray()
        self.usb_ = device
        self.write_endpoint = write_endpoint
        self.read_endpoint = read_endpoint
        self.chunk_size = chunk_size
        self.timeout = timeout
        try:
            device.detach_kernel_driver(interface) # type: ignore
        except usb.core.USBError:
            pass

    def read(self, size: int|None = -1) -> bytes:
        '''We interpret negative sizes to mean everything until a timeout of
        zero returns nothing.'''
        if size is None:
            size = 64
        if size >= 0:
            if len(self.in_buffer) == 0:
                self.in_buffer += cast(bytes, self.usb_.read( # type: ignore
                    self.read_endpoint, self.chunk_size, self.timeout))

            count = min(size, len(self.in_buffer))
            ret = bytes(self.in_buffer[:count])
            del self.in_buffer[:count]
            return ret

        timeout = self.timeout
        try:
            while True:
                r = cast(bytes, self.usb_.read( # type: ignore
                    self.read_endpoint, self.chunk_size, timeout))
                if r == b'':
                    break
                self.in_buffer += r
                timeout = 0
        except usb.core.USBTimeoutError:
            pass

        ret = bytes(self.in_buffer)
        self.in_buffer.clear()
        return ret

    def write(self, b: ByteString) -> int:
        if len(b) > self.chunk_size:
            b = b[:self.chunk_size]
        return self.usb_.write(         # type: ignore
            self.write_endpoint, b, self.timeout)
