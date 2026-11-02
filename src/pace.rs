use crate::model::Track;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Features {
    pub bpm: f32,
    pub energy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Chill,
    Focus,
    Workout,
    Party,
    Sad,
}

impl Mood {
    pub const ALL: [Mood; 5] = [Mood::Chill, Mood::Focus, Mood::Workout, Mood::Party, Mood::Sad];

    pub fn label(self) -> &'static str {
        match self {
            Mood::Chill => "Chill",
            Mood::Focus => "Focus",
            Mood::Workout => "Workout",
            Mood::Party => "Party",
            Mood::Sad => "Sad",
        }
    }

    pub fn target(self) -> Features {
        match self {
            Mood::Chill => Features { bpm: 85.0, energy: 0.30 },
            Mood::Focus => Features { bpm: 100.0, energy: 0.40 },
            Mood::Workout => Features { bpm: 140.0, energy: 0.85 },
            Mood::Party => Features { bpm: 124.0, energy: 0.80 },
            Mood::Sad => Features { bpm: 72.0, energy: 0.25 },
        }
    }

    pub fn query(self) -> &'static str {
        match self {
            Mood::Chill => "chill relaxing songs",
            Mood::Focus => "focus instrumental songs",
            Mood::Workout => "workout energy songs",
            Mood::Party => "party dance songs",
            Mood::Sad => "sad emotional songs",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reaction {
    Liked,
    Completed,
    SkippedEarly,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub bpm: f32,
    pub energy: f32,
    pub samples: u32,
}

const MAX_BPM_STEP: f32 = 14.0;
const MOOD_WEIGHT: f32 = 0.6;

impl Profile {
    pub fn empty() -> Self {
        Self { bpm: 0.0, energy: 0.0, samples: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.samples == 0
    }

    pub fn learn(&mut self, features: Features, reaction: Reaction) {
        if self.is_empty() {
            if reaction != Reaction::SkippedEarly {
                *self = Self { bpm: features.bpm, energy: features.energy, samples: 1 };
            }
            return;
        }
        let weight = match reaction {
            Reaction::Liked => 0.45,
            Reaction::Completed => 0.25,
            Reaction::SkippedEarly => -0.12,
        };
        let bpm_target = nearest_octave(features.bpm, self.bpm);
        let bpm_step = ((bpm_target - self.bpm) * weight).clamp(-MAX_BPM_STEP, MAX_BPM_STEP);
        self.bpm = (self.bpm + bpm_step).clamp(50.0, 200.0);
        self.energy = (self.energy + (features.energy - self.energy) * weight).clamp(0.0, 1.0);
        self.samples += 1;
    }

    pub fn target(&self, mood: Option<Mood>) -> Option<Features> {
        match (mood, self.is_empty()) {
            (Some(mood), true) => Some(mood.target()),
            (Some(mood), false) => {
                let goal = mood.target();
                Some(Features {
                    bpm: goal.bpm * MOOD_WEIGHT + self.bpm * (1.0 - MOOD_WEIGHT),
                    energy: goal.energy * MOOD_WEIGHT + self.energy * (1.0 - MOOD_WEIGHT),
                })
            }
            (None, false) => Some(Features { bpm: self.bpm, energy: self.energy }),
            (None, true) => None,
        }
    }
}

pub fn nearest_octave(bpm: f32, reference: f32) -> f32 {
    [bpm * 0.5, bpm * 2.0]
        .into_iter()
        .filter(|candidate| ((candidate - reference).abs() / reference.max(1.0)) < 0.12)
        .min_by(|a, b| (a - reference).abs().total_cmp(&(b - reference).abs()))
        .unwrap_or(bpm)
}

pub fn reaction_for(listened_secs: f64, duration_secs: Option<f64>) -> Option<Reaction> {
    let duration = duration_secs.filter(|d| *d > 20.0)?;
    let fraction = listened_secs / duration;
    if fraction >= 0.8 {
        Some(Reaction::Completed)
    } else if listened_secs < 25.0 && fraction < 0.25 {
        Some(Reaction::SkippedEarly)
    } else {
        None
    }
}

pub fn distance(features: Features, target: Features) -> f32 {
    let bpm = nearest_octave(features.bpm, target.bpm);
    let tempo = ((bpm - target.bpm).abs() / 40.0).min(1.5);
    let octave_penalty = if (bpm - features.bpm).abs() > 1.0 { 0.15 } else { 0.0 };
    let energy = (features.energy - target.energy).abs() * 1.6;
    tempo * 0.4 + energy * 0.6 + octave_penalty
}

pub fn title_hint(title: &str) -> Option<Features> {
    let lower = title.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    if has(&["lofi", "lo-fi", "slowed", "reverb", "acoustic", "unplugged", "sleep", "piano", "ambient"]) {
        Some(Features { bpm: 78.0, energy: 0.25 })
    } else if has(&["remix", "dj ", "edm", "bass", "club", "party", "workout", "mass", "dance", "sped up"]) {
        Some(Features { bpm: 128.0, energy: 0.8 })
    } else {
        None
    }
}

pub fn rank(
    tracks: Vec<Track>,
    target: Option<Features>,
    known: &dyn Fn(u64) -> Option<Features>,
    recent_artists: &[String],
) -> Vec<Track> {
    let Some(target) = target else {
        return tracks;
    };
    let mut scored: Vec<(f32, usize, Track)> = tracks
        .into_iter()
        .enumerate()
        .map(|(position, track)| {
            let mut score = match known(track.id) {
                Some(features) => 1.0 - distance(features, target),
                None => match title_hint(&track.title) {
                    Some(hint) => 0.9 * (1.0 - distance(hint, target)),
                    None => 0.35,
                },
            };
            let artist = track.artist.split([',', '&']).next().unwrap_or("").trim().to_lowercase();
            if !artist.is_empty() && recent_artists.contains(&artist) {
                score -= 0.15;
            }
            (score, position, track)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, track)| track).collect()
}

const FRAME: usize = 512;
const FRAME_HOP: usize = 256;

fn fft_magnitudes(frame: &[f32], window: &[f32], out: &mut [f32]) {
    let n = FRAME;
    let mut re: Vec<f32> = vec![0.0; n];
    let mut im: Vec<f32> = vec![0.0; n];
    for index in 0..n {
        let target = (index as u32).reverse_bits() >> (32 - n.trailing_zeros());
        re[target as usize] = frame[index] * window[index];
    }
    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let step = -2.0 * std::f32::consts::PI / size as f32;
        for start in (0..n).step_by(size) {
            for k in 0..half {
                let (sin, cos) = (step * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + half);
                let tr = re[b] * cos - im[b] * sin;
                let ti = re[b] * sin + im[b] * cos;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        size *= 2;
    }
    for (bin, slot) in out.iter_mut().enumerate() {
        *slot = (re[bin] * re[bin] + im[bin] * im[bin]).sqrt();
    }
}

fn unit(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn analyze(samples: &[f32], sample_rate: u32) -> Option<Features> {
    if sample_rate < 4000 || samples.len() < sample_rate as usize * 4 {
        return None;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_square.sqrt();
    if rms < 0.002 {
        return None;
    }
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos())
        .collect();
    let bins = FRAME / 2 + 1;
    let bin_hz = sample_rate as f32 / FRAME as f32;
    let mut previous = vec![0.0f32; bins];
    let mut current = vec![0.0f32; bins];
    let mut flux: Vec<f32> = Vec::new();
    let (mut centroid_sum, mut high_sum, mut counted) = (0.0f32, 0.0f32, 0usize);
    let mut start = 0;
    let mut first = true;
    while start + FRAME <= samples.len() {
        fft_magnitudes(&samples[start..start + FRAME], &window, &mut current);
        let total: f32 = current.iter().sum::<f32>() + 1e-9;
        centroid_sum += current.iter().enumerate().map(|(bin, m)| m * bin as f32 * bin_hz).sum::<f32>() / total;
        high_sum += current.iter().enumerate().filter(|(bin, _)| *bin as f32 * bin_hz > 2000.0).map(|(_, m)| m).sum::<f32>() / total;
        counted += 1;
        if !first {
            let rise: f32 = current.iter().zip(previous.iter()).map(|(now, before)| (now - before).max(0.0)).sum();
            flux.push(rise / total);
        }
        first = false;
        std::mem::swap(&mut previous, &mut current);
        start += FRAME_HOP;
    }
    if flux.len() < 16 {
        return None;
    }
    let centroid = centroid_sum / counted as f32;
    let high = high_sum / counted as f32;
    let flux_mean = flux.iter().sum::<f32>() / flux.len() as f32;
    let db = 20.0 * rms.log10();
    let energy = 0.40 * unit((flux_mean - 0.10) / 0.26)
        + 0.25 * unit((high - 0.06) / 0.30)
        + 0.25 * unit((centroid - 500.0) / 1200.0)
        + 0.10 * unit((db + 30.0) / 20.0);

    let mut envelope: Vec<f32> = flux.iter().map(|value| value - flux_mean).collect();
    let smoothed: Vec<f32> = (0..envelope.len())
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(envelope.len() - 1);
            envelope[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32
        })
        .collect();
    envelope = smoothed;
    let hop_secs = FRAME_HOP as f32 / sample_rate as f32;
    let mut best = (f32::MIN, 120.0f32);
    let mut bpm = 60.0f32;
    while bpm <= 180.0 {
        let lag = 60.0 / bpm / hop_secs;
        let whole = lag.floor() as usize;
        let frac = lag - whole as f32;
        if whole + 1 < envelope.len() {
            let count = envelope.len() - whole - 1;
            let mut sum = 0.0;
            for index in 0..count {
                let shifted = envelope[index + whole] * (1.0 - frac) + envelope[index + whole + 1] * frac;
                sum += envelope[index] * shifted;
            }
            let prior = (-((bpm / 120.0).log2() / 0.6).powi(2) / 2.0).exp();
            let value = sum / count as f32 * prior;
            if value > best.0 {
                best = (value, bpm);
            }
        }
        bpm += 0.5;
    }
    Some(Features { bpm: best.1, energy: unit(energy) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn clicks(bpm: f32, seconds: f32, rate: u32, amplitude: f32) -> Vec<f32> {
        let total = (seconds * rate as f32) as usize;
        let period = (60.0 / bpm * rate as f32) as usize;
        let burst = (rate as f32 * 0.03) as usize;
        let mut out = vec![0.0f32; total];
        let mut at = 0;
        while at + burst < total {
            for k in 0..burst {
                let decay = 1.0 - k as f32 / burst as f32;
                out[at + k] = amplitude * decay * (k as f32 * 0.9).sin();
            }
            at += period;
        }
        out
    }

    fn track(title: &str, artist: &str, id: u64) -> Track {
        let mut track = Track::local(Path::new("/m/a.mp3"));
        track.title = title.into();
        track.artist = artist.into();
        track.id = id;
        track
    }

    #[test]
    fn finds_the_tempo_of_a_click_track() {
        for bpm in [90.0f32, 120.0, 140.0] {
            let found = analyze(&clicks(bpm, 20.0, 11025, 0.5), 11025).expect("features").bpm;
            assert!((found - bpm).abs() / bpm < 0.05, "expected {bpm}, found {found}");
        }
    }

    fn pad(seconds: f32, rate: u32) -> Vec<f32> {
        (0..(seconds * rate as f32) as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let swell = 0.6 + 0.4 * (t * 0.7).sin();
                swell * (0.3 * (t * 2.0 * std::f32::consts::PI * 110.0).sin() + 0.2 * (t * 2.0 * std::f32::consts::PI * 165.0).sin())
            })
            .collect()
    }

    fn bright_beat(seconds: f32, rate: u32) -> Vec<f32> {
        let total = (seconds * rate as f32) as usize;
        let beat = 60.0 / 140.0;
        let mut noise = 12345u32;
        (0..total)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let phase = (t % beat) / beat;
                let pulse = 0.35 + 0.65 * (1.0 - phase).powi(3);
                noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
                let hiss = (noise >> 16) as f32 / 32768.0 - 1.0;
                0.5 * pulse * hiss + 0.2 * (t * 2.0 * std::f32::consts::PI * 100.0).sin()
            })
            .collect()
    }

    #[test]
    fn energy_separates_a_soft_pad_from_a_bright_beat_and_silence_has_none() {
        let calm = analyze(&pad(20.0, 11025), 11025).unwrap().energy;
        let driven = analyze(&bright_beat(20.0, 11025), 11025).unwrap().energy;
        assert!(driven > calm + 0.25, "calm {calm} driven {driven}");
        assert_eq!(analyze(&vec![0.0; 11025 * 10], 11025), None);
        assert_eq!(analyze(&[0.1; 100], 11025), None);
    }

    #[test]
    #[ignore = "needs dumped sample files in DUMP_DIR"]
    fn calibration_on_dumped_clips() {
        let dir = std::env::var("DUMP_DIR").expect("DUMP_DIR");
        let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        names.sort();
        for path in names {
            let bytes = std::fs::read(&path).unwrap();
            let samples: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
            let features = analyze(&samples, 11025).unwrap();
            println!("CAL {:6} bpm={:6.1} energy={:.2}", path.file_stem().unwrap().to_string_lossy(), features.bpm, features.energy);
        }
    }

    #[test]
    fn profile_follows_liked_songs_and_backs_away_from_skips() {
        let mut profile = Profile::empty();
        assert!(profile.target(None).is_none());
        profile.learn(Features { bpm: 80.0, energy: 0.3 }, Reaction::SkippedEarly);
        assert!(profile.is_empty(), "a skip alone teaches nothing");
        profile.learn(Features { bpm: 80.0, energy: 0.3 }, Reaction::Completed);
        for _ in 0..6 {
            profile.learn(Features { bpm: 130.0, energy: 0.8 }, Reaction::Liked);
        }
        assert!(profile.bpm > 110.0 && profile.energy > 0.6, "{profile:?}");
        let before = profile.bpm;
        profile.learn(Features { bpm: 80.0, energy: 0.2 }, Reaction::SkippedEarly);
        assert!(profile.bpm > before, "skipping a slow song should push the pace up");
    }

    #[test]
    fn pace_changes_gradually_not_in_jumps() {
        let mut profile = Profile::empty();
        profile.learn(Features { bpm: 70.0, energy: 0.2 }, Reaction::Completed);
        let start = profile.bpm;
        profile.learn(Features { bpm: 180.0, energy: 1.0 }, Reaction::Liked);
        assert!(profile.bpm - start <= MAX_BPM_STEP + 0.01);
    }

    #[test]
    fn mood_blends_with_the_session_and_stands_alone_when_new() {
        let empty = Profile::empty();
        assert_eq!(empty.target(Some(Mood::Workout)), Some(Mood::Workout.target()));
        let mut profile = Profile::empty();
        profile.learn(Features { bpm: 80.0, energy: 0.3 }, Reaction::Completed);
        let blended = profile.target(Some(Mood::Workout)).unwrap();
        assert!(blended.bpm > 100.0 && blended.bpm < 140.0);
        assert!(blended.energy > 0.3 && blended.energy < 0.85);
    }

    #[test]
    fn half_and_double_time_count_as_the_same_pace() {
        let target = Features { bpm: 70.0, energy: 0.4 };
        let doubled = distance(Features { bpm: 140.0, energy: 0.4 }, target);
        let far = distance(Features { bpm: 105.0, energy: 0.4 }, target);
        assert!(doubled < far);
        assert_eq!(nearest_octave(140.0, 70.0), 70.0);
    }

    #[test]
    fn reactions_come_from_how_much_was_heard() {
        assert_eq!(reaction_for(200.0, Some(220.0)), Some(Reaction::Completed));
        assert_eq!(reaction_for(8.0, Some(220.0)), Some(Reaction::SkippedEarly));
        assert_eq!(reaction_for(90.0, Some(220.0)), None);
        assert_eq!(reaction_for(5.0, None), None);
        assert_eq!(reaction_for(5.0, Some(10.0)), None);
    }

    #[test]
    fn ranking_prefers_matching_pace_and_penalises_repeated_artists() {
        let slow = track("Quiet piano night", "A", 1);
        let fast = track("Club bass remix", "B", 2);
        let plain = track("Unlabelled song", "C", 3);
        let tracks = vec![slow.clone(), plain.clone(), fast.clone()];
        let energetic = Some(Mood::Workout.target());
        let order = rank(tracks.clone(), energetic, &|_| None, &[]);
        assert_eq!(order[0].id, 2);
        assert_eq!(order[2].id, 1);
        let calm = Some(Mood::Chill.target());
        assert_eq!(rank(tracks.clone(), calm, &|_| None, &[])[0].id, 1);
        let known = |id: u64| (id == 3).then_some(Features { bpm: 142.0, energy: 0.82 });
        assert_eq!(rank(tracks.clone(), energetic, &known, &[])[0].id, 3);
        let twins = vec![track("One", "Same Artist", 10), track("Two", "Other Artist", 11)];
        let similar = |_: u64| Some(Features { bpm: 138.0, energy: 0.84 });
        assert_eq!(rank(twins.clone(), energetic, &similar, &[])[0].id, 10);
        let after_repeat = rank(twins, energetic, &similar, &["same artist".to_string()]);
        assert_eq!(after_repeat[0].id, 11);
        assert_eq!(rank(tracks.clone(), None, &|_| None, &[]).iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 3, 2]);
    }
}
