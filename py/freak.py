#!/usr/bin/python3

assert __name__ == '__main__'

import freak.lmk05318b_plan as lmk05318b_plan
import freak.lmk05318b_util as lmk05318b_util
import freak.message as message
import freak.ublox_util as ublox_util

import argparse, struct, sys, uuid

from freak.freak_util import Device

argp = argparse.ArgumentParser(description='GPS Freak utility')

argp.add_argument('-s', '--serial',
                  help='Serial port for GPS comms (default is direct USB)')
argp.add_argument('-b', '--baud', type=int,
                  help='Baud rate for serial (Default is no change)')

subp = argp.add_subparsers(
    dest='command', metavar='COMMAND', required=True, help='Command')

freq = subp.add_parser(
    'freq', aliases=['frequency'], help='Program/report frequencies',
    description='''Program or frequencies If a list of frequencies is given,
    these are programmed to the device.  With no arguments, report the
    current device frequencies.''')
freq.add_argument('FREQ', nargs='*', help='Frequencies in MHz')

info = subp.add_parser(
    'info', help='Basic device info', description='Basic device info')

planp = subp.add_parser(
    'plan', help='Frequency planning',
    description='''Compute and print a frequency plan without programming it
    to the device.''')
planp.add_argument('FREQ', nargs='+', help='Frequencies in MHz')

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
    freqs = lmk05318b_util.make_freq_list(args.FREQ, False)
    plan = lmk05318b_plan.plan(freqs)
    lmk05318b_util.report_plan(plan, False)

elif args.command == 'freq':
    if len(args.FREQ) != 0:
        lmk05318b_util.do_freq(device, args.FREQ, False)
    else:
        lmk05318b_util.report_freq(device, False)

elif args.command == 'reboot':
    dev = device.get_usb()
    # Leave these in reset until the reboot takes effect.
    message.command(dev, message.LMK05318B_PDN, b'\0')
    message.command(dev, message.GPS_RESET, b'\0')
    dev.write(0x03, message.frame(message.CPU_REBOOT, b''))

elif args.command == 'cpu-reset':
    # Just send the command blindly, no response.
    device.get_usb().write(
        0x03, message.frame(message.CPU_REBOOT, b''))

elif args.command in ('clock', 'lmk05318b'):
    lmk05318b_util.run_command(args, device, args.clock)

elif args.command in ('gps', 'ublox'):
    ublox_util.run_command(args, device, args.gps)

else:
    print(args)
    assert False, f'This should never happen {args.command}'
