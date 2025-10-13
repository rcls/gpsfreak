#!/usr/bin/python3

assert __name__ == '__main__'

import freak.lmk05318b_plan as lmk05318b_plan
import freak.lmk05318b_util as lmk05318b_util
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

planp = subp.add_parser(
    'plan', help='Frequency planning',
    description='''Compute and print a frequency plan without programming it
    to the device.''')
planp.add_argument('FREQ', nargs='+', help='Frequencies in MHz')

freq = subp.add_parser(
    'freq', aliases=['frequency'], help='Program/report frequencies',
    description='''Program or frequencies If a list of frequencies is given,
    these are programmed to the device.  With no arguments, report the
    current device frequencies.''')
freq.add_argument('FREQ', nargs='*', help='Frequencies in MHz')

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

clock_p = subp.add_parser(
    'clock', aliases=['lmk05318b'], help='LMK05318b clock-gen maintenance.',
    description='''Sub-commands for operating on the LMK05318b clock generator
    chip.''',
    epilog='''Note that these sub-commands use the internal LMK05318b numbering
    of channels, not those on the device case.  Top-level commands
    use the device case numbering.''')
lmk05318b_util.add_to_argparse(clock_p, dest='clock', metavar='SUB-COMMAND')

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

# Ping with a UUID and check that we get the same one back...
#message.ping(dev, bytes(str(uuid.uuid4()), 'ascii'))

if args.command == 'info':
    dev = message.get_device()
    do_info(dev)

elif args.command == 'plan':
    freqs = lmk05318b_util.make_freq_list(args.FREQ, False)
    plan = lmk05318b_plan.plan(freqs)
    lmk05318b_util.report_plan(plan, False)

elif args.command == 'freq':
    if len(args.FREQ) != 0:
        lmk05318b_util.do_freq(args.FREQ, False)
    else:
        lmk05318b_util.report_freq(False)

elif args.command == 'reboot':
    # Just send the command blindly, no response.
    dev = message.get_device()
    dev.write(0x03, message.frame(message.CPU_REBOOT, b''))

elif args.command == 'gps-reset':
    dev = message.get_device()
    do_reset_line(dev, message.GPS_RESET)

elif args.command == 'lmk-pdn':
    dev = message.get_device()
    do_reset_line(dev, message.LMK05318B_PDN)

elif args.command in ('clock', 'lmk05318b'):
    lmk05318b_util.run_command(args, args.clock)

else:
    print(args)
    assert False, f'This should never happen {args.command}'
