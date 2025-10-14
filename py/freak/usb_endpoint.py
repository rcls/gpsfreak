'''Expose a usb endpoint (pair) as a io.RawIOBase object.

We use this as an alternative way to access the serial port, because doing
everything via pyusb makes it trivial to correlate the serial port and command
endpoint—every OS serial driver goes out of its way to make it difficult to
get a handle on the underlying USB device.'''

import io
import usb.core

class USBEndpointIO(io.IOBase):
    in_buffer: bytearray = bytearray()
    usb: usb.core.Device
    write_endpoint: int
    read_endpoint: int
    timeout: float
    chunk_size: int

    def __init__(self, device, interface, write_endpoint, read_endpoint,
                 timeout=1000, chunk_size = 64):
        self.usb = device
        self.write_endpoint = write_endpoint
        self.read_endpoint = read_endpoint
        self.chunk_size = chunk_size
        self.timeout = timeout
        try:
            device.detach_kernel_driver(interface)
        except usb.core.USBError:
            pass

    def read(self, size: int|None = -1) -> bytes:
        '''We interpret negative sizes to mean everything until a timeout of
        zero returns nothing.'''
        if size is None:
            size = 64
        if size >= 0:
            if len(self.in_buffer) == 0:
                self.in_buffer += self.usb.read(
                    self.read_endpoint, self.chunk_size, self.timeout)

            count = min(size, len(self.in_buffer))
            ret = bytes(self.in_buffer[:count])
            del self.in_buffer[:count]
            return ret

        timeout = self.timeout
        try:
            while True:
                r = self.usb.read(self.read_endpoint, self.chunk_size, timeout)
                if r == b'':
                    break
                self.in_buffer += r
                timeout = 0
        except usb.core.USBTimeoutError:
            pass

        ret = bytes(self.in_buffer)
        self.in_buffer.clear()
        return ret

    def write(self, b: bytes) -> int:
        if len(b) > self.chunk_size:
            b = b[:self.chunk_size]
        return self.usb.write(self.write_endpoint, b, self.timeout)
