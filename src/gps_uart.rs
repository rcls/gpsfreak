// USART2
// TX on pin 19 PA8 (USART2 AF4, USART3 AF13)
// RX on pin 18 PB15 (USART2 AF13, USART1 AF4, LPUART1, AF8)

use crate::cpu::{barrier, interrupt};
use crate::vcell::{UCell, VCell};

use stm32h503::USART2 as UART;
use stm32h503::Interrupt::USART2 as INTERRUPT;

// NOTE: In safe boot we seem to need a UU training sequence to get the baud
// rate sane.
const BAUD: u32 = 9600;
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR < 65536);

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let uart  = unsafe {&*UART::ptr()};

    rcc.APB1LENR.modify(|_,w| w.USART2EN().set_bit());

    gpioa.AFRH.modify(|_,w| w.AFSEL8().B_0x4());
    gpiob.AFRH.modify(|_,w| w.AFSEL15().B_0xD());
    gpioa.MODER.modify(|_,w| w.MODE8().B_0x2());
    gpiob.MODER.modify(|_,w| w.MODE15().B_0x2());

    uart.BRR.write(|w| w.bits(BRR));

    uart.CR3.write(|w| w.RXFTCFG().bits(5).RXFTIE().set_bit());
    uart.CR1.write(
        |w|w.FIFOEN().set_bit().RE().set_bit().RXFNEIE().set_bit()
            .TE().set_bit().UE().set_bit());

    // FIXME interrupt::set_priority(INTERRUPT, 0xff);
    interrupt::enable(INTERRUPT);
}

fn isr() {
    let uart = unsafe {&*UART::ptr()};
    let isr = uart.ISR.read();
    let cr1 = uart.CR1.read();
    // Attempt to clear all the interrupts.
    uart.ICR.write(|w| w.bits(isr.bits()));
    //crate::dbg!("UART ISR = {:#010x}", isr.bits());

    let rxfne = isr.RXFNE().bit();
    // Whenever RXFT is set, or we reach idle, push the data through.
    if isr.RXFT().bit() || rxfne && isr.IDLE().bit() && cr1.IDLEIE().bit() {
        // Drain!
        let mut n: usize = 0;
        let mut buf = [0; 8];
        loop {
            buf[n] = uart.RDR.read().bits() as u8;
            n += 1;
            if n == 8 || !uart.ISR.read().RXFNE().bit() {
                break;
            }
        }
        crate::uart_debug::write_bytes(&buf[..n]);
    }

    uart.CR1.write(
        |w| w.bits(cr1.bits()).RXFNEIE().bit(!rxfne).IDLEIE().bit(rxfne));

    // Now handle TX...
    if isr.TXFE().bit() {
        // Refill.
        let mut r = TX.r.read();
        let w = TX.w.read();
        loop {
            if r == w {
                uart.CR1.modify(|_,w| w.TXFEIE().clear_bit());
                break;
            }
            let byte = *TX.buf[r as usize].as_ref();
            uart.TDR.write(|w| w.bits(byte as u32));
            r = r.wrapping_add(1) & BUF_MASK;
            if !uart.ISR.read().TXFNF().bit() {
                break;
            }
        }
        TX.r.write(r);
    }
}

impl crate::cpu::VectorTable {
    pub const fn gps_uart(&mut self) -> &mut Self {
        self.isr(INTERRUPT, isr)
    }
}

type Index = u16;
const BUF_SIZE: usize = 1024;
const BUF_MASK: Index = (BUF_SIZE - 1) as Index;
const _: () = assert!(BUF_SIZE <= Index::MAX as usize + 1);
const _: () = assert!(BUF_SIZE & BUF_SIZE - 1 == 0);

struct GpsTx {
    w: VCell<Index>,
    r: VCell<Index>,
    buf: [UCell<u8>; BUF_SIZE],
}

static TX: GpsTx = GpsTx{w: VCell::new(0), r: VCell::new(0),
                          buf: [const {UCell::new(0)}; BUF_SIZE]};

pub fn send_byte(byte: u8) {
    //crate::dbgln!("GPS send_byte");
    let w = TX.w.read();
    let next_w = w.wrapping_add(1) & BUF_MASK;
    if TX.r.read() == next_w {
        return; // Full.
    }
    barrier();
    unsafe {*TX.buf[w as usize].as_mut() = byte};
    barrier();
    TX.w.write(next_w);
    let uart = unsafe {&*UART::ptr()};
    uart.CR1.modify(|_,w| w.TXFEIE().set_bit());
}
