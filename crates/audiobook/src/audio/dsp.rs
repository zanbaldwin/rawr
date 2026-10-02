use std::time::Duration;

/// Copy `samples` with a linear fade of `length` samples at each edge, so
/// joins between pieces do not click. The first and last samples become 0.
pub fn fade(samples: &[i16], length: usize) -> Vec<i16> {
    let length = length.min(samples.len() / 2).max(1);
    let last = samples.len().saturating_sub(1);
    samples
        .iter()
        .enumerate()
        .map(|(index, &sample)| {
            let edge = index.min(last - index);
            if edge >= length { sample } else { (f64::from(sample) * edge as f64 / length as f64) as i16 }
        })
        .collect()
}

/// Finds a sound that starts after the speech has ended.
///
/// Long ElevenLabs batches (seen in Korean) can end with a silence and then
/// the start of a new sound after the last word. Cutting at the start of that
/// silence removes the stray sound and keeps the pause.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailingSound {
    /// Level below which a 10 ms window is silent, in dBFS.
    pub silence_dbfs: f64,
    /// Level above which a 10 ms window after the silence is a sound, in dBFS.
    pub sound_dbfs: f64,
    /// Shortest silence that counts.
    pub min_silence: Duration,
}

impl Default for TrailingSound {
    /// The values proven in purr: −60 dBFS silence for 100 ms, then a sound above −50 dBFS.
    fn default() -> Self {
        Self {
            silence_dbfs: -60.0,
            sound_dbfs: -50.0,
            min_silence: Duration::from_millis(100),
        }
    }
}

impl TrailingSound {
    /// The sample where such a silence starts, searching from sample `from`
    /// (the end of the last spoken letter or digit), or `None` if the audio
    /// after `from` has no silence followed by a sound.
    pub fn find(&self, samples: &[i16], sample_rate: u32, from: usize) -> Option<usize> {
        let window = (sample_rate / 100).max(1) as usize;
        let needed = self.min_silence.as_millis().div_ceil(10).max(1) as usize;
        let mut run_start = None;
        let mut run_length = 0;
        let mut silence = None;
        for start in (from.min(samples.len())..samples.len()).step_by(window) {
            let frame = &samples[start..(start + window).min(samples.len())];
            let energy = frame.iter().map(|&sample| f64::from(sample).powi(2)).sum::<f64>() / frame.len() as f64;
            let level = 20.0 * (energy.sqrt().max(1.0) / 32_768.0).log10();
            if level < self.silence_dbfs {
                run_start.get_or_insert(start);
                run_length += 1;
                if run_length >= needed {
                    silence = silence.or(run_start);
                }
            } else {
                if level > self.sound_dbfs && silence.is_some() {
                    return silence;
                }
                run_start = None;
                run_length = 0;
            }
        }
        None
    }
}
