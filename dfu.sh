#!/bin/bash

sudo dfu-util -e -E 1

sleep 1

set -e -x

OUT=./target/thumbv8m.main-none-eabihf/release

arm-none-eabi-objcopy -O binary "$OUT"/freak.elf "$OUT"/freak.bin

sudo dfu-util -a 0 -s 0x08000000:leave -R -D "$OUT"/freak.bin
