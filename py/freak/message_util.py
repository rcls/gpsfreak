
from freak.message import Recipient, command

from argparse import Namespace
from typing import Any

# !@#$@# argparse, what is the type of a subparser?
def add_reset_command(subp: Any, unit: str) -> None:
    reset = subp.add_parser(
        'reset', help=f'Reset the {unit}.',
        description=f'''Reset the {unit} via its reset pin.''',
        epilog='''With no argument, the reset line is asserted low for 1
        millisecond.  Alternatively, the arguments allow you to leave it in (or
        out) of reset.

        Note that this is liable to leave your device unusable until it
        is recovered e.g., by power cycling.''')
    reset.add_argument('-0', '--assert', action='store_true',
                       help='Assert reset line (low).')
    reset.add_argument('-1', '--deassert', action='store_true',
                       help='De-assert reset line (high).')

def do_reset_line(dev: Recipient, code: int, args: Namespace|None) -> None:
    payload = b'\x02'
    if args is not None:
        assrt = getattr(args, 'assert')
        deassert = args.deassert
        if assrt and not deassert:
            payload = b'\x00'
        elif not assrt and deassert:
            payload = b'\x01'

    command(dev, code, payload)
