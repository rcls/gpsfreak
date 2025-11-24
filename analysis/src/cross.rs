#![allow(mixed_script_confusables)]

use std::fs::File;
use memmap2::Mmap;

#[allow(unused)]
mod util;
use util::*;

use rustfft::num_complex::{Complex64, c64};

fn get_hilbert(path: &String) -> Vec<Complex64> {
    let mmap = unsafe {Mmap::map(&File::open(path).unwrap()).unwrap()};
    let len = mmap.len();

    let nice = be_nice_to_fft(len - 4096 - 10_000_000);
    let mmap = &mmap[len - nice ..];
    let len = mmap.len();

    let mut data = complexify_bytes(&mmap);

    fft_forward(&mut data);
    raised_cosine_ends(&mut data[..len/2], 10000);
    for p in &mut data[len/2..] {
        *p = c64(0.0, 0.0);
    }
    fft_inverse(&mut data);
    for p in data.iter_mut() {
        *p /= p.norm();
    }
    data
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let p1 = args[1].clone();
    let p2 = args[2].clone();
    //let frequency: f64 = args[3].parse().unwrap();
    let h1 = std::thread::spawn(move || get_hilbert(&p1));
    let h2 = std::thread::spawn(move || get_hilbert(&p2));

    let mut data1 = h1.join().unwrap();
    let data2 = h2.join().unwrap();

    assert_eq!(data1.len(), data2.len());

    for (p, a) in data1.iter_mut().zip(data2.iter()) {
        *p *= a.conj();
    }
    spectrum(&mut data1, 0.0);
}