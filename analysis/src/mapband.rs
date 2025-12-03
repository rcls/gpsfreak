#![allow(mixed_script_confusables)]

use std::fs::File;
use memmap2::Mmap;

#[allow(unused)]
mod util;
use util::*;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    let file = File::open(&args[1]).unwrap();

    let mmap = unsafe {Mmap::map(&file).unwrap()};
    let len = mmap.len();
    let nice = be_nice_to_fft(len - 13_000_000);
    eprintln!("FFT nice : {nice} / {len} (lost {})", len - nice);
    let bytes = &mmap[len - nice ..];

    if args.len() <= 2 {
        println!("Freq est. {}", freq_estimate(bytes));
        return;
    }

    let frequency = args[2].parse().unwrap();

    if false {
        cycle_lengths(bytes);
    }
    else if false {
        phases(bytes, frequency);
    }
    else {
        let mut data = downshift_bytes(bytes, frequency);
        spectrum(&mut data, frequency);
    }
}
