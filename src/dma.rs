
pub use stm_common::dma::DMA_Channel;

pub type Dma = stm32h503::gpdma1::RegisterBlock;
pub type Channel = stm32h503::gpdma1::c::C;

pub fn dma() -> &'static Dma {unsafe {&*stm32h503::GPDMA1::ptr()}}

pub use stm_common::dma::Flat;
