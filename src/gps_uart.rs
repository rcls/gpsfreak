// USART2
// TX on pin 19 PA8 (USART2 AF4, USART3 AF13)
// RX on pin 18 PB15 (USART2 AF13, USART1 AF4, LPUART1, AF8)

use crate::cpu::interrupt;
use crate::vcell::UCell;

use stm32h503::GPDMA1 as DMA;
use stm32h503::USART2 as UART;
use stm32h503::Interrupt::USART2 as INTERRUPT;
use stm32h503::Interrupt::GPDMA1_CH0 as DMA_INTERRUPT;

// NOTE: In safe boot we seem to need a UU training sequence to get the baud
// rate sane.
const BAUD: u32 = 9600;
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR < 65536);

/// For serial TX we use DMA.
const DMA_CHANNEL: usize = 0;

/// USART2 DMA TX.
const TX_DMA_REQ: u8 = 24;

#[derive(Copy)]
#[derive_const(Clone)]
struct RxPartial32 {
    part: u32
}

static RX_PARTIAL: UCell<RxPartial32> = Default::default();

impl const Default for RxPartial32 {
    fn default() -> Self {RxPartial32::new()}
}

impl RxPartial32 {
    const fn new() -> RxPartial32 {RxPartial32{part: 128}}
    fn push(&mut self, b: u8) -> Option<u32> {
        let updated = (self.part << 8) + b as u32;
        if self.part < 0x80000000 {
            // This is not the last byte, just push it.
            self.part = updated;
            None
        }
        else {
            self.part = 128;
            Some(u32::from_be(updated))
        }
    }
    fn flush(&mut self) -> (u32, usize) {
        let part = self.part;
        self.part = 128;
        if part < 0x00800000 {
            if part < 0x00008000 {
                (0, 0)
            }
            else {
                (part & 0xff, 1)
            }
        }
        else if part < 0x80000000 {
            (u32::from_be(part << 16), 2)
        }
        else {
            (u32::from_be(part << 8), 3)
        }
    }
}

// DMA : EN deasserts in HW when finished.
// Suspend/resume. SUSP & SUSPF.
// Abort suspended via CxCR RESET. (Clears EN).
// FIFO mode.
// u32->u8 conversion:
// can unpack, might need byte reorder? Manual says little endian :-)
// TRIGM=00 I think.
// In packing mode (PAM[1]=1) BNDT appears to be number of dest bytes.

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let uart  = unsafe {&*UART::ptr()};

    rcc.AHB1ENR.modify(|_,w| w.GPDMA1EN().set_bit());
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

    interrupt::set_priority(INTERRUPT, 0xff);
    interrupt::set_priority(DMA_INTERRUPT, 0xff);
    interrupt::enable(INTERRUPT);
    interrupt::enable(DMA_INTERRUPT);
}

/// Assumes that the DMA channel is idle.
/// Len must fit in 16 bits.
pub fn dma_tx(data: *const u8, len: usize) {
    let dma  = unsafe {&*DMA ::ptr()};
    let uart = unsafe {&*UART::ptr()};
    let ch = &dma.C[DMA_CHANNEL];
    ch.DAR.write(|w| w.bits(uart.TDR.as_ptr() as u32));
    ch.SAR.write(|w| w.bits(data as u32));
    ch.BR2.write(|w| w.bits(len as u32));
    // Pack mode, source increment, dest u8, source u32.
    ch.TR1.write(|w| w.PAM().bits(2).SINC().set_bit().SDW_LOG2().B_0x2());
    ch.TR2.write(|w| w.REQSEL().bits(TX_DMA_REQ));

    // TODO - check if TC gets set on error halts!
    ch.CR.write(|w| w.EN().set_bit().TCIE().set_bit());
}

fn uart_isr() {
    let uart = unsafe {&*UART::ptr()};
    let isr = uart.ISR.read();
    let cr1 = uart.CR1.read();
    // Attempt to clear all the interrupts.
    uart.ICR.write(|w| w.bits(isr.bits()));
    //crate::dbg!("UART ISR = {:#010x}", isr.bits());

    let rxfne = isr.RXFNE().bit();
    // Whenever RXFT is set, or we reach idle, push the data through.
    // TODO - do we need IDLE interrupt?  We could just poll from SOF.
    if isr.RXFT().bit() || rxfne && isr.IDLE().bit() && cr1.IDLEIE().bit() {
        // Drain the FIFO.
        let part = unsafe {RX_PARTIAL.as_mut()};
        loop {
            if let Some(word) = part.push(uart.RDR.read().bits() as u8) {
                // Send the word on to the USB.
                crate::usb::serial_tx_push32(word);
            }

            if !uart.ISR.read().RXFNE().bit() {
                break;
            }
        }
    }

    uart.CR1.write(
        |w| w.bits(cr1.bits()).RXFNEIE().bit(!rxfne).IDLEIE().bit(rxfne));
}

fn dma_isr() {
    let dma = unsafe {&*DMA::ptr()};
    let ch = &dma.C[DMA_CHANNEL];
    let sr = ch.SR.read();
    ch.FCR.write(|w| w.bits(sr.bits()));      // Clear the interrupts.

    if sr.bits() & 0x7f00 != 0 {
        // We completed a transfer, or it errored.
        let buf_end = ch.DAR.read().bits() as *const u8;
        // This may kick off the next transfer.
        crate::usb::serial_rx_done(buf_end);
    }
}

pub fn serial_rx_flush() -> (u32, usize) {
    unsafe {RX_PARTIAL.as_mut()}.flush()
}

impl crate::cpu::VectorTable {
    pub const fn gps_uart(&mut self) -> &mut Self {
        self.isr(INTERRUPT, uart_isr).isr(DMA_INTERRUPT, dma_isr)
    }
}
