MEMORY
{
  FLASH(RX) : ORIGIN = 0x08000000, LENGTH = 128K
  RAM(WX) : ORIGIN = 0x20000000, LENGTH = 32K
}

end_of_ram = ORIGIN(RAM) + LENGTH(RAM);
EXTERN(main);
ENTRY(main);

SECTIONS
{
  .text : {
     KEEP(*(.vectors*)),
     *(.text*)
     *(SORT_BY_ALIGNMENT(.rodata*))
  } > FLASH
  .data : {
     __data_start = .;
     *(SORT_BY_ALIGNMENT(.data*))
     __data_end = .;
  } > RAM AT>FLASH
  .bss (NOLOAD) : {
     __bss_start = .;
     *(SORT_BY_ALIGNMENT(.bss*))
     __bss_end = .;
     *(SORT_BY_ALIGNMENT(.noinit*))
     . += 2K; /* Stack reservation. */
  } > RAM
  .stack_sizes (INFO): {
     KEEP(*(.stack_sizes));
  }
}

__rom_data_start = LOADADDR(.data);
