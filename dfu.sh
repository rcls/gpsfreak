#!/bin/bash

#sudo dfu-util -e -E 1
if test -n "$1"
then
    ./freak -n $1 reset --dfu
    sleep 1
fi

set -e -x

cargo b

OUT=./target/thumbv8m.main-none-eabihf/release

arm-none-eabi-objcopy -O binary "$OUT"/freak.elf "$OUT"/freak.bin

sudo dfu-util -d 0483:df11 -a 0 -s 0x08000000:leave -R -D "$OUT"/freak.bin
