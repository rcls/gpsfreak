
use rustfft::FftPlanner;
use rustfft::num_complex::{Complex64, c64};

use std::f64::consts::PI;

const SAMPLES_PER_SEC: f64 = 100e6;

pub fn fft_forward(data: &mut [Complex64]) {
    let mut planner = FftPlanner::new();
    let fplan = planner.plan_fft_forward(data.len());
    fplan.process(data);
}

pub fn fft_inverse(data: &mut [Complex64]) {
    let mut planner = FftPlanner::new();
    let fplan = planner.plan_fft_inverse(data.len());
    fplan.process(data);
}

pub fn cycle_lengths(data: &[u8]) {
    let mut last_rise = 0.0f64;
    let mut last_b = 0xff;
    let mut last_gap = 0;
    for (i, &b) in data.iter().enumerate() {
        if last_b < 0xb0 && b >= 0xb0 {
            let rise = i as f64
                + (0xb0 - last_b) as f64 / (b - last_b) as f64;
            let gap = rise - last_rise;
            last_rise = rise;
            if gap < 6.11 || gap > 6.44 {
                println!("{i} {gap} {}", i - last_gap);
                last_gap = i;
            }
        }
        last_b = b;
    }
}

pub fn be_nice_to_fft(len: usize) -> usize {
    let mut best = 1;
    let mut twos = 1;
    while twos <= len {
        let mut threes = twos;
        while threes <= len {
            if threes > best {
                best = threes;
            }
            threes *= 3;
        }
        twos *= 2;
    }
    best
}

pub fn rotate(i: usize, b: f64, frequency: f64) -> Complex64 {
    let cycles = i as f64 * frequency * (1.0 / SAMPLES_PER_SEC);
    let θ = (cycles.round() - cycles) * (2.0 * PI);
    let (s, c) = θ.sin_cos();
    b * c64(c, -s)
}

pub fn downshift_bytes(bytes: &[u8], frequency: f64) -> Vec<Complex64> {
    let total: u64 = bytes.iter().map(|&x| x as u64).sum();
    let mean = total as f64 / bytes.len() as f64;
    bytes.iter().enumerate().map(
        |(i, &b)| rotate(i, b as f64 - mean, frequency)).collect()
}

pub fn complexify_bytes(bytes: &[u8]) -> Vec<Complex64> {
    let total: u64 = bytes.iter().map(|&x| x as u64).sum();
    let mean = total as f64 / bytes.len() as f64;
    bytes.iter().map(|&b| (b as f64 - mean).into()).collect()
}

pub fn time_raised_cosine(data: &mut [Complex64]) {
    let ω = 2.0 * PI / data.len() as f64;
    for (i, d) in data.iter_mut().enumerate() {
        *d *= 1.0 - (ω * i as f64).cos();
    }
}

pub fn raised_cosine_ends(data: &mut [Complex64], half_width: usize) {
    let ω = PI / half_width as f64;
    let len = data.len();
    data[0] = 0.0f64.into();
    for i in 1 .. half_width {
        let scale = 0.5 - (ω * i as f64).cos() * 0.5;
        data[i] *= scale;
        data[len - i] *= scale;
    }
}

//pub fn spectrum(bytes: &[u8], frequency: f64) {
//    let mut data = downshift_bytes(bytes, frequency);
pub fn spectrum(data: &mut [Complex64], frequency: f64) {
    time_raised_cosine(data);
    fft_forward(data);

    let scale = SAMPLES_PER_SEC / data.len() as f64;
    let height = data[0].norm_sqr();
    println!("{},1,1,fundamental,{frequency},{height}", 0.5 * scale);
    for i in 1..10 {
        eprintln!("peak,{i},{},{}", data[i].norm_sqr() / height,
                  data[data.len() - i].norm_sqr() / height);
    }

    let mut start = 1;
    loop {
        let end = start + 1.max(start / 1000);
        if end > data.len() / 2 {
            break;
        }
        // Calc max and mean power in range.
        let mut max: f64 = 0.0;
        let mut total = 0.0;
        for i in start..end {
            let p1 = data[i].norm_sqr() / height;
            let p2 = data[data.len() - i].norm_sqr() / height;
            max = max.max(p1).max(p2);
            total += p1 + p2;
        }
        let mean = total / (2 * (end - start)) as f64;
        let f = (start + end - 1) as f64 * 0.5 * scale;
        println!("{f},{mean},{max}");
        start = end;
    }
}

pub fn phase_unwind(data: &[Complex64],
                    middle: usize, half_width: usize) -> Vec<f64> {
    let width = 2 * half_width;

    let mut low = Vec::<Complex64>::new();
    low.resize(width, 0.0f64.into());

    assert!(middle + half_width <= data.len());
    let top;
    if middle == 0 {
        top = data.len();
        assert!(top >= width);
    }
    else {
        top = middle;
        assert!(top >= half_width);
        assert!(top + half_width <= data.len());
    }
    // Select items at ±half_width and apply a raised cosine window.  This is as
    // frequency domain window, so extremes of array are near center (zero)
    // frequency.
    let scale = PI / half_width as f64;
    low[0] = 2.0 * data[middle];
    for i in 1 .. half_width {
        // Map 0..half_width onto 0..PI.
        let c = 1.0 + (i as f64 * scale).cos();
        low[i] = c * data[middle + i];
        low[width - i] = c * data[top - i];
    }

    fft_inverse(&mut low);

    let mut result = Vec::new();
    let mut cycles: f64 = 0.0;

    for v in low {
        let θ = v.arg() * (0.5 / PI);
        let mut next = cycles.round() + θ;
        if next - cycles > 0.5 {
            next = next - 1.0;
        }
        else if next - cycles < -0.5 {
            next = next + 1.0;
        }
        assert!((cycles - next).abs() < 0.5);
        result.push(next);
        cycles = next;
    };

    result
}

pub fn phases(bytes: &[u8], frequency: f64) {
    let mut data = downshift_bytes(bytes, frequency);

    fft_forward(&mut data);

    let width = (data.len() as f64 * (10e3 / SAMPLES_PER_SEC)) as usize;
    for cycles in phase_unwind(&data, 0, width) {
        println!("{cycles}");
    }
}

pub fn freq_estimate(bytes: &[u8]) -> f64 {
    let total: u64 = bytes.iter().map(|&x| x as u64).sum();
    let mean = total as f64 / bytes.len() as f64;
    let mut data: Vec::<Complex64> = bytes.iter().map(
        |&b| (b as f64 - mean).into()).collect();

    fft_forward(&mut data);

    let half_width = 10000;
    let mut maxi = 0;
    let mut max = 0.0;
    for i in half_width .. data.len() / 2 {
        let ns = data[i].norm_sqr();
        if ns >= max {
            max = ns;
            maxi = i;
        }
    }

    let frequency = maxi as f64 * SAMPLES_PER_SEC / data.len() as f64;
    eprintln!("Max at {maxi}, frequency {frequency}");
    let phases = phase_unwind(&data, maxi, 10000);

    let start = phases.len() / 10;
    let end = phases.len() - start;
    let count = end - start + 1;

    let total: f64 = phases[start ..= end].iter().sum();
    let mean = total / count as f64;
    let center = (start + end) as f64 * 0.5;
    let mut sigma_iθ = 0.0;
    let mut sigma_ii = 0.0;
    for i0 in start ..= end {
        let i = i0 as f64 - center;
        let θ = phases[i0] - mean;
        sigma_iθ += i * θ;
        sigma_ii += i * i;
    }
    let total_change = sigma_iθ / sigma_ii * phases.len() as f64;
    eprintln!("Total change {total_change}");
    let frequency = frequency
        + total_change * SAMPLES_PER_SEC / data.len() as f64;

    eprintln!("Adjusted frequency {frequency}");

    frequency
}
