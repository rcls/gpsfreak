// USART2
// TX on pin 19 PA8 (USART2 AF4, USART3 AF13)
// RX on pin 18 PB15 (USART2 AF13, USART1 AF4, LPUART1, AF8)

use crate::cpu;
use crate::cpu::interrupt;
use crate::dma::DMA_Channel;
use crate::vcell::{UCell, VCell};

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

/// Set to true to loopback our own data instead of processing received data.
const LOOPBACK: bool = false;

#[derive(Copy)]
#[derive_const(Clone)]
struct RxPartial32 {
    part: u32
}

/// Partial 32-bit word received from the UART.  We need to aggregate into
/// 32 bit words to match the USBSRAM. :-(
static RX_PARTIAL: UCell<RxPartial32> = Default::default();

/// Data toggle, if any, of the data buffer with in-flight DMA.  Note that this
/// is just used for consistency checking - we could actually get rid of it.
static DMA_TOGGLE: UCell<Option<bool>> = Default::default();

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

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let dma   = unsafe {&*DMA ::ptr()};
    let uart  = unsafe {&*UART::ptr()};

    rcc.AHB1ENR.modify(|_,w| w.GPDMA1EN().set_bit());
    rcc.APB1LENR.modify(|_,w| w.USART2EN().set_bit());

    gpioa.AFRH.modify(|_,w| w.AFSEL8().B_0x4());
    gpiob.AFRH.modify(|_,w| w.AFSEL15().B_0xD());
    gpioa.MODER.modify(|_,w| w.MODE8().B_0x2());
    gpiob.MODER.modify(|_,w| w.MODE15().B_0x2());

    uart.BRR.write(|w| w.bits(BRR));

    uart.CR3.write(|w| w.RXFTCFG().bits(5).RXFTIE().set_bit().DMAT().set_bit());
    uart.CR1.write(
        |w|w.FIFOEN().set_bit().RE().set_bit().RXFNEIE().set_bit()
            .TE().set_bit().UE().set_bit());

    let ch = &dma.C[DMA_CHANNEL];
    ch.writes_to(uart.TDR.as_ptr() as *mut u8, TX_DMA_REQ);

    interrupt::enable_priority(INTERRUPT, 0xff);
    interrupt::enable_priority(DMA_INTERRUPT, 0xff);
}

static BAUD_RATE: VCell<u32> = VCell::new(9600);

pub fn set_baud_rate(baud: u32) -> bool {
    let uart  = unsafe {&*UART::ptr()};
    // We need to disable the UART to udpate the baud rate.
    // FIXME - validate.  FIXME - allow us to recover the baud rate!
    // FIXME - use the prescalar also.
    let brr = (cpu::CPU_FREQ * 2 + baud) / (baud * 2);
    // We are called from the USB ISR, which is the same priority as our ISRs.
    // So there should be no interrupt to race with.
    let config = uart.CR1.read().bits();
    uart.CR1.write(|w| w.UE().clear_bit());
    uart.BRR.write(|w| w.bits(brr));
    uart.CR1.write(|w| w.bits(config));
    BAUD_RATE.write(baud);
    true
}

pub fn get_baud_rate() -> u32 {
    BAUD_RATE.read()
}

/// Returns false if the DMA is busy, or true if the DMA is started.
/// Len must fit in 16 bits.
pub fn dma_tx(data: *const u8, len: usize, toggle: bool) -> bool {
    let dma_toggle = unsafe {DMA_TOGGLE.as_mut()};
    if let Some(_) = *dma_toggle {
        return false;                   // Busy
    }
    *dma_toggle = Some(toggle);

    if LOOPBACK {
        for b in unsafe {core::slice::from_raw_parts(data, len)} {
            if let Some(word) = unsafe{RX_PARTIAL.as_mut()}.push(*b) {
                crate::usb::serial_tx_push32(word);
            }
        }
    }
    let dma  = unsafe {&*DMA ::ptr()};
    let ch = &dma.C[DMA_CHANNEL];

    ch.write(data as usize, len);

    crate::cpu::barrier();

    true
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
            if LOOPBACK {
                uart.RDR.read(); // Ignore the data.
            }
            else if let Some(word) = part.push(uart.RDR.read().bits() as u8) {
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

    // Be care to read CR after SR.
    let cr = ch.CR.read();

    if !cr.EN().bit() && sr.bits() & 0x7f00 != 0 {
        // We completed a transfer, or it errored.
        let dma_toggle = unsafe {DMA_TOGGLE.as_mut()};
        if let Some(toggle) = *dma_toggle {
            *dma_toggle = None;
            // This may kick off the next transfer.
            crate::usb::serial_rx_done(toggle);
        }
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
