//! Bunyi alarm "bip-bip-bip" yang dibuat di memori (tanpa file audio) dan
//! diputar berulang oleh PlaySound bawaan Windows. Buffer dilepas saat berhenti.

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{
    PlaySoundW, SND_ASYNC, SND_LOOP, SND_MEMORY, SND_NODEFAULT, SND_SYSTEM,
};

const RATE: u32 = 22_050;

pub struct Alarm {
    /// WAV yang sedang diputar; harus hidup selama PlaySound memakainya.
    wav: Option<Vec<u8>>,
}

impl Alarm {
    pub fn new() -> Self {
        Self { wav: None }
    }

    pub fn start(&mut self) {
        self.stop();
        let wav = self.wav.insert(build_wav());
        // SAFETY: buffer WAV disimpan di `self.wav` dan baru dilepas setelah
        // PlaySound dihentikan di `stop`.
        unsafe {
            let _ = PlaySoundW(
                PCWSTR(wav.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_LOOP | SND_NODEFAULT | SND_SYSTEM,
            );
        }
    }

    pub fn stop(&mut self) {
        if self.wav.is_some() {
            // SAFETY: menghentikan suara yang sedang diputar proses ini.
            unsafe {
                let _ = PlaySoundW(PCWSTR::null(), None, Default::default());
            }
            self.wav = None;
        }
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        self.stop();
    }
}

/// PCM 16-bit mono: tiga nada 880 Hz (140 ms, jeda 90 ms) lalu hening 650 ms.
fn build_wav() -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::with_capacity(RATE as usize * 13 / 10);
    for _ in 0..3 {
        tone(&mut samples, 880.0, 140);
        silence(&mut samples, 90);
    }
    silence(&mut samples, 650);

    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // ukuran chunk fmt
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte per detik
    wav.extend_from_slice(&2u16.to_le_bytes()); // byte per sampel
    wav.extend_from_slice(&16u16.to_le_bytes()); // bit per sampel
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

fn tone(buf: &mut Vec<i16>, freq: f32, ms: u32) {
    let n = RATE * ms / 1000;
    let fade = (RATE * 6 / 1000) as f32; // 6 ms supaya tidak ada bunyi "klik"
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let envelope = (i.min(n - 1 - i) as f32 / fade).min(1.0);
        let wave = (std::f32::consts::TAU * freq * t).sin() * 0.8 + (2.0 * std::f32::consts::TAU * freq * t).sin() * 0.2;
        buf.push((wave * envelope * 0.45 * i16::MAX as f32) as i16);
    }
}

fn silence(buf: &mut Vec<i16>, ms: u32) {
    buf.resize(buf.len() + (RATE * ms / 1000) as usize, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_matches_data() {
        let wav = build_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data_len);
        // Sekitar 1,3 detik per pola
        assert!((data_len / 2) as u32 > RATE * 12 / 10);
    }
}
