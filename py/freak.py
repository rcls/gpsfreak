#!/usr/bin/python3

assert __name__ == '__main__'

import freak.message as message

import argparse
import struct
import usb
import uuid

argp = argparse.ArgumentParser(description='LMK05318b utility')
subp = argp.add_subparsers(
    dest='command', metavar='COMMAND', required=True, help='Command')

info = subp.add_parser(
    'info', help='Basic device info', description='Basic device info')

reboot = subp.add_parser(
    'reboot', help='Reboot device CPU', description='Reboot device CPU')

gps_reset = subp.add_parser(
    'gps-reset', help='Reset GPS unit', description='Reset GPS unit')
gps_reset.add_argument('-0', '--assert', action='store_true',
                       help='Assert reset line (low)')
gps_reset.add_argument('-1', '--deassert', action='store_true',
                       help='De-assert reset line (high)')

lmk_reset = subp.add_parser(
    'lmk-pdn', help='LMK05318b power down', description='LMK05318b power down')
lmk_reset.add_argument('-0', '--assert', action='store_true',
                       help='Assert PDN line (low)')
lmk_reset.add_argument('-1', '--deassert', action='store_true',
                       help='De-assert PDN line (high)')

def do_info():
    pv = message.retrieve(dev, message.GET_PROTOCOL_VERSION)
    print('Protocol Version:', struct.unpack('<I', pv.payload)[0])

    serial = message.retrieve(dev, message.GET_SERIAL_NUMBER)
    try:
        sn = serial.payload.decode()
    except:
        sn = str(serial)
    print('Device serial No:', sn)

    result = message.retrieve(dev, message.TMP117_READ, b'\x02\x00\x00')
    assert len(result.payload) == 2
    temp = struct.unpack('>H', result.payload)[0] / 128
    print('Int. temperature:', temp, '°C')

def do_reset_line(command):
    print(args)
    assrt = getattr(args, 'assert')
    desrt = args.deassrt
    if assrt and not desrt:
        payload = b'\x00'
    elif not assrt and desrt:
        payload = b'\x01'
    else:
        payload = b'\x02'
    message.command(command, payload)

args = argp.parse_args()

dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)

# Ping with a UUID and check that we get the same one back...
uuid = bytes(str(uuid.uuid4()), 'ascii')
reply = message.command(dev, message.PING, uuid)
assert reply.payload == uuid

if args.command == 'info':
    do_info()

elif args.command == 'reboot':
    # Just send the command blindly, no response.
    dev.write(0x03, frame(message.REBOOT, b''))

elif args.command == 'gps-reset':
    do_reset_line(message.GPS_RESET)

elif args.command == 'lmk-pdn':
    do_reset_line(message.LMK_RESET)

else:
    assert False, 'This should never happen'
