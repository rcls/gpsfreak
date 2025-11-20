'''pyserial is a bit broken, in that it's .read sematics are very inconvenient
for variable sized messages.  Instead, a quick little helper that just gives
us io.FileIO semantics.

You can create a serhelper.Serial object for a serial port.

All the functionality is exposed as plain functions, so that if you end up with
some other io.IOBase object for a serial port, then you can still use the
functions.

All the serial-specific functionality silently does nothing if used with
something that is not a serial port.  This is useful for code that can e.g.,
use a plain file or a socket instead of a serial port.

We only support raw mode on Posix ttys.  The historical Posix baggage for
teletype support is ignored.'''

import io

class Serial(io.FileIO):
    def __init__(self, path: str, speed: int|None = None):
        io.FileIO.__init__(self, path, 'r+b')
        configure(self, speed)

    def writeall(self, b: bytes) -> int:
        return writeall(self, b)

    def flushread(self) -> None:
        flushread(self)

# IOBase appears not to have a write() method?
def writeall(f: io.IOBase, b: bytes) -> int:
    mv = memoryview(b)
    done = 0
    while done < len(mv):
        progress: int = f.write(mv[done:])
        if progress == 0:
            raise EOFError()
        done += progress
    return done

def flushread(f: io.IOBase) -> None:
    if f.isatty():
        import termios
        termios.tcflush(f.fileno(), termios.TCIFLUSH)

def configure(f: io.RawIOBase, speed: int|None) -> None:
    if not f.isatty():
        return
    import termios
    from termios import (
        BRKINT, CS8, CSIZE, ECHO, ECHONL, ICANON, ICRNL, IEXTEN, IGNBRK, IGNCR,
        INLCR, ISIG, ISTRIP, IXON, OPOST, PARENB, PARMRK, VMIN, VTIME)
    attr = termios.tcgetattr(f.fileno())
    attr[0] &= ~(                                        # iflag
        IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON)
    attr[1] &= ~OPOST                                    # oflag
    attr[2] &= ~(CSIZE | PARENB)                         # cflag
    attr[2] |= CS8                                       # cflag
    attr[3] &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN) # lflag
    if speed:
        attr[4] = speed                                  # ispeed
        attr[5] = speed                                  # ospeed
    attr[6][VMIN] = 0
    attr[6][VTIME] = 10
    termios.tcsetattr(f.fileno(), termios.TCSAFLUSH, attr)
