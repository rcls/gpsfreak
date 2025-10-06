import io
import os
import termios

class Serial(io.FileIO):
    def __init__(self, path: str, speed: int|None = None):
        io.FileIO.__init__(self, path, 'r+b')
        makeraw(self, speed)

    def writeall(self, b):
        writeall(self, b)

    def flushread(self):
        flushread(self)

def writeall(f: io.IOBase, b) -> int:
    mv = memoryview(b)
    done = 0
    while done < len(mv):
        progress: int = f.write(mv[done:])
        if progress == 0:
            raise EOFError()
        done += progress
    return done

def flushread(f: io.IOBase):
    if f.isatty():
        termios.tcflush(f, termios.TCIFLUSH)

def makeraw(f: io.IOBase, speed: int|None):
    if not f.isatty():
        return
    from termios import (
        BRKINT, CS8, CSIZE, ECHO, ECHONL, ICANON, ICRNL, IEXTEN, IGNBRK, IGNCR,
        INLCR, ISIG, ISTRIP, IXON, OPOST, PARENB, PARMRK, VMIN, VTIME)
    attr = termios.tcgetattr(f)
    attr[0] &= ~(                                        # iflag
        IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON)
    attr[1] &= ~OPOST                                    # oflag
    attr[2] &= ~(CSIZE | PARENB)                         # cflag
    attr[2] |= CS8                                       # cflag
    attr[3] &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN) # lflag
    print(f'Pre speed {attr[4]} {attr[5]}')
    if speed:
        attr[4] = speed                                  # ispeed
        attr[5] = speed                                  # ospeed
    print(f'Set speed {attr[4]} {attr[5]}')
    attr[6][VMIN] = 0
    attr[6][VTIME] = 10
    termios.tcsetattr(f, termios.TCSAFLUSH, attr)
