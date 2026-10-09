//! Synthesised notification and tick sounds. The output stream is not `Send`,
//! so it lives on its own thread and is driven over a channel.

use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::f32::consts::TAU;
use std::sync::mpsc::{channel, Sender};

const RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug)]
pub enum Sound {
    FocusEnd,
    BreakEnd,
    TickFocus,
    TickBreak,
}

#[derive(Clone)]
pub struct Audio {
    tx: Sender<(Sound, f32)>,
}

impl Audio {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<(Sound, f32)>();
        std::thread::spawn(move || {
            // Opened lazily so a missing device doesn't cost anything until needed,
            // and retried on the next sound if it fails.
            let mut out: Option<(OutputStream, rodio::OutputStreamHandle)> = None;
            for (sound, volume) in rx {
                if out.is_none() {
                    out = OutputStream::try_default().ok();
                }
                let Some((_, handle)) = &out else { continue };
                match Sink::try_new(handle) {
                    Ok(sink) => {
                        sink.set_volume(volume);
                        sink.append(SamplesBuffer::new(1, RATE, samples(sound)));
                        sink.detach();
                    }
                    Err(_) => out = None,
                }
            }
        });
        Self { tx }
    }

    pub fn play(&self, sound: Sound, volume: f32) {
        let _ = self.tx.send((sound, volume));
    }
}

fn samples(sound: Sound) -> Vec<f32> {
    match sound {
        // Rising three-note chime: time for a break.
        Sound::FocusEnd => chime(&[(523.25, 0.0), (659.25, 0.16), (783.99, 0.32)], 1.6),
        // Falling two-note chime, twice: back to work.
        Sound::BreakEnd => chime(
            &[(880.0, 0.0), (659.25, 0.18), (880.0, 0.55), (659.25, 0.73)],
            1.8,
        ),
        Sound::TickFocus => tick(1900.0),
        Sound::TickBreak => tick(1300.0),
    }
}

/// Bell-like notes: a fundamental plus two quieter partials with exponential decay.
fn chime(notes: &[(f32, f32)], seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut buf = vec![0.0f32; n];
    for &(freq, start) in notes {
        let offset = (start * RATE as f32) as usize;
        for (i, s) in buf.iter_mut().enumerate().skip(offset) {
            let t = (i - offset) as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let env = attack * (-t * 4.5).exp();
            *s += env
                * 0.22
                * ((TAU * freq * t).sin()
                    + 0.35 * (TAU * freq * 2.0 * t).sin()
                    + 0.12 * (TAU * freq * 3.01 * t).sin());
        }
    }
    buf
}

fn tick(freq: f32) -> Vec<f32> {
    let n = (0.035 * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (-t * 220.0).exp() * 0.35 * (TAU * freq * t).sin()
        })
        .collect()
}
