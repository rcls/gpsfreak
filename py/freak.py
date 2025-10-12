#!/usr/bin/python3

assert __name__ == '__main__'

import freak.message as message

import argparse
import struct
import uuid

from freak.message import Device

argp = argparse.ArgumentParser(description='GPS Freak utility')
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

def do_info(dev: Device) -> None:
    pv = message.retrieve(dev, message.GET_PROTOCOL_VERSION)
    print('Protocol Version:', struct.unpack('<I', pv.payload)[0])

    serial = message.get_serial_number(dev)
    try:
        sn = serial.decode()
    except:
        sn = serial.hex(' ')
    print('Device serial No:', sn)

    result = message.tmp117_read(dev, 0, 2)
    assert len(result) == 2
    temp = struct.unpack('>H', result)[0] / 128
    print('Int. temperature:', temp, '°C')

def do_reset_line(dev: Device, command: int) -> None:
    print(args)
    assrt = getattr(args, 'assert')
    desrt = args.deassert
    if assrt and not desrt:
        payload = b'\x00'
    elif not assrt and desrt:
        payload = b'\x01'
    else:
        payload = b'\x02'
    message.command(dev, command, payload)

args = argp.parse_args()

dev = message.get_device()

# Ping with a UUID and check that we get the same one back...
message.ping(dev, bytes(str(uuid.uuid4()), 'ascii'))

if args.command == 'info':
    do_info(dev)

elif args.command == 'reboot':
    # Just send the command blindly, no response.
    dev.write(0x03, message.frame(message.CPU_REBOOT, b''))

elif args.command == 'gps-reset':
    do_reset_line(dev, message.GPS_RESET)

elif args.command == 'lmk-pdn':
    do_reset_line(dev, message.LMK05318B_PDN)

else:
    assert False, 'This should never happen'
