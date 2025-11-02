#!/usr/bin/python3

assert __name__ == '__main__'

import sys
if sys.version_info < (3, 10):
    print('Your python version {sys.version} is too old. ',
          'Please get 3.10 or later')

import freak.lmk05318b_plan as lmk05318b_plan
import freak.lmk05318b_util as lmk05318b_util
import freak.config as config
import freak.message as message
import freak.ublox_util as ublox_util

from freak.freak_util import Device
from typing import Tuple

import argparse, struct, uuid

argp = argparse.ArgumentParser(description='GPS Freak utility')

subp = argp.add_subparsers(
    dest='command', metavar='COMMAND', required=True, help='Command')

lmk05318b_util.add_freq_commands(subp, 'output', 'device output connector')

# FIX - put in utils.
def key_value(s: str) -> Tuple[str, str]:
    if not '=' in s:
        raise ValueError('Key/value pairs must be in the form KEY=VALUE')
    k, v =  s.split('=', 1)
    return k, v
key_value.__name__ = 'key=value pair'

drive = subp.add_parser(
    'drive', help='Report output drive', description='Set/Report output drive.')
drive.add_argument('DRIVE', type=key_value, nargs='*', metavar='OUT=DRIVE',
                   help='Output and drive type / strength')

info = subp.add_parser(
    'info', help='Basic device info', description='Basic device info')

save = subp.add_parser(
    'save', help='Save clock gen config to flash.',
    description='''Save running frequency configuration to CPU flash.
    Other configuration saved in flash, such as GPS, will be preserved.''')
save.add_argument('-n', '--dry-run', action='store_true', default=False,
                  help="Don't actually write to flash.")

configp = subp.add_parser(
    'config', help='Saved configuration maintenance',
    description='Saved configuration maintenance')
config.add_to_argparse(configp, dest='config', metavar='SUB-COMMAND')

reboot = subp.add_parser(
    'reboot', help='Cold restart entire device',
    description='''Cold restart entire device.  This is equivalent to
    power-cycling.''')

cpu_reset = subp.add_parser(
    'cpu-reset', help='Reset device CPU', description='Reset device CPU')

clock_p = subp.add_parser(
    'clock', aliases=['lmk05318b'], help='LMK05318b clock-gen maintenance',
    description='''Sub-commands for operating on the LMK05318b clock generator
    chip.''',
    epilog='''Note that these sub-commands use the internal LMK05318b numbering
    of channels, not those on the device case.  Top-level commands
    use the device case numbering.''')
lmk05318b_util.add_to_argparse(clock_p, dest='clock', metavar='SUB-COMMAND')

gps_p = subp.add_parser(
    'gps', aliases=['ublox'], help='UBlox GPS maintenance',
    description='''Sub-commands for operation on the UBlox GPS module.''')
ublox_util.add_to_argparse(gps_p, dest='gps', metavar='SUB-COMMAND')

def do_info(device: Device) -> None:
    dev = device.get_usb()
    # Ping with a UUID and check that we get the same one back...
    message.ping(dev, bytes(str(uuid.uuid4()), 'ascii'))

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

    pv = message.retrieve(dev, message.GET_PROTOCOL_VERSION)
    print('Protocol Version:', struct.unpack('<I', pv.payload)[0])

if len(sys.argv) < 2:
    argp.print_help()
    sys.exit(1)

args = argp.parse_args()

device = Device(args)

if args.command == 'info':
    do_info(device)

elif args.command == 'plan':
    target = lmk05318b_util.make_freq_target(args, False)
    plan = lmk05318b_plan.plan(target)
    data = lmk05318b_util.freq_make_data(plan)
    lmk05318b_util.report_freq(data, False, False)

elif args.command == 'freq':
    if len(args.FREQ) != 0:
        lmk05318b_util.do_freq(device, args, False)
    else:
        lmk05318b_util.report_live_freq(device, False)

elif args.command == 'drive':
    if args.DRIVE:
        lmk05318b_util.do_drive_out(device, args.DRIVE)
    else:
        lmk05318b_util.report_driveout(device)

elif args.command == 'save':
    config.save_config(device, save_ubx=False, save_lmk = True,
                       dry_run = args.dry_run)

elif args.command == 'config':
    config.run_command(args, device, args.config)

elif args.command == 'reboot':
    dev = device.get_usb()
    # Leave these in reset until the reboot takes effect.
    message.command(dev, message.LMK05318B_PDN, b'\0')
    message.command(dev, message.GPS_RESET, b'\0')
    dev.write(0x03, message.frame(message.CPU_REBOOT, b'')) # type: ignore

elif args.command == 'cpu-reset':
    # Just send the command blindly, no response.
    device.get_usb().write( # type: ignore
        0x03, message.frame(message.CPU_REBOOT, b''))

elif args.command in ('clock', 'lmk05318b'):
    lmk05318b_util.run_command(args, device, args.clock)

elif args.command in ('gps', 'ublox'):
    ublox_util.run_command(args, device, args.gps)

else:
    print(args)
    assert False, f'This should never happen {args.command}'
