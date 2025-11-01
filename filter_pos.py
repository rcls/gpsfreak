#!/usr/bin/python3

import argparse
import csv
import re
import sys

BOM_FIRST_LINE = None

PREFIX_ROTATE = [
    ('MSOP-', -90),
    ('QFN-', -90),
    ('QFN10_TG5032CGN', -90),
    ('SOIC-', -90),
    ('SOT-', 180),
    ('SIL0008C', 180),
    ('Texas_S-PWSON', -90)]

def read_bom(BOM):
    global BOM_FIRST_LINE
    f = csv.reader(open(BOM, newline=''))
    BOM_FIRST_LINE = next(f)
    assert BOM_FIRST_LINE[0] == 'Value'

    return list(f)

def refset(bom):
    refs = []
    for L in bom:
        refs.extend(L[1].split(','))
    return set(refs)

def map_package(package):
    if ':' in package:
        package = package.rsplit(':', 1)[-1]
    if package.startswith('Diode_SMD:D_SO'):
        package = package.removeprefix('Diode_SMD:D_')
    m = re.match(r'(R|C|L|LED)_(\d\d\d\d)_(\d\d\d\d)Metric', package)
    if m:
        package = m.group(2)
    return package

def read_cpl(IN, refs):
    lines = ['Designator,Mid X,Mid Y,Layer,Rotation']
    f = csv.reader(open(IN, newline=''))
    first = next(f)
    assert first[0] == 'Ref', f'{first}'

    for L in f:
        #print(L)
        ref, val, package, posx, posy, rot, side = L
        #assert ref in refs, f'{ref}'
        if not ref in refs:
            continue

        rot = float(rot)
        for prefix, rotate in PREFIX_ROTATE:
            if package.startswith(prefix):
                rot += rotate
            if rot > 180:
                rot -= 360
            if rot <= -180:
                rot += 360
        lines.append(f'{ref},{posx}mm,{posy}mm,{side},{rot}')
    return lines

def write_cpl(OUT, lines):
    outf = open(OUT, 'w')
    for L in lines:
        print(L, file=outf)

def filter_cpl(IN, BOM, OUT):
    refs = refset(read_bom(BOM))
    lines = read_cpl(IN, refs)
    write_cpl(OUT, lines)

def filter_bom(IN, OUT):
    lines = read_bom(IN)
    w = csv.writer(open(OUT, 'w'))
    assert BOM_FIRST_LINE is not None
    w.writerow(BOM_FIRST_LINE)
    for l in lines:
        l[2] = map_package(l[2])
        w.writerow(l)

argp = argparse.ArgumentParser(description='BOM & CPL processing')
subp = argp.add_subparsers(dest='command', required=True)

cpl_parser = subp.add_parser('cpl', description='Filter CPL file')
cpl_parser.add_argument('IN')
cpl_parser.add_argument('BOM')
cpl_parser.add_argument('OUT')

bom_parser = subp.add_parser('bom', description='Filter BOM file')
bom_parser.add_argument('IN')
bom_parser.add_argument('OUT')

args = argp.parse_args()

if args.command == 'cpl':
    filter_cpl(args.IN, args.BOM, args.OUT)

if args.command == 'bom':
    filter_bom(args.IN, args.OUT)
