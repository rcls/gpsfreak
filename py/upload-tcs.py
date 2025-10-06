#!/usr/bin/python3

from freak import message, tics
import sys
import usb

from usb.core import USBTimeoutError

blocks = tics.read_tcs_file(sys.argv[1])
bundles = blocks.bundle(ro = False, max_block = 30)
messages = tics.make_i2c_transactions(bundles)

print(messages)

dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)

# Flush any stale data.
message.flush(dev)

ping_resp = message.transact(dev, 0, b'This is a test')

print(ping_resp)
assert ping_resp.code == 0x0080
assert ping_resp.payload == b'This is a test'

for msg in messages:
    print('Send', msg)
    reply = message.transact(dev, msg.code, msg.payload)
    print(reply)

# Now do the reset dance.
print('Reset')
reply = message.transact(dev, message.LMK05318B_WRITE, bytes((0, 12, 0x12)))
print(reply)

reply = message.transact(dev, message.LMK05318B_WRITE, bytes((0, 12, 0x02)))
print(reply)
