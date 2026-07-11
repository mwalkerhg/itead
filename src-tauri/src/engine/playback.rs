use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub struct PlaybackTrackInfo {
    pub wav_path: String,
    pub volume_db: f32,
}

pub(crate) struct PlaybackSession {
    is_playing: Arc<AtomicBool>,
    reader_handle: Option<std::thread::JoinHandle<Result<rtrb::Producer<f32>>>>,
    position_samples: Arc<AtomicU64>,
    total_frames: u64,
    sample_rate: u32,
}

struct LoadedTrack {
    samples: Vec<f32>,
    volume_linear: f32,
}

impl PlaybackSession {
    pub fn start(
        tracks: Vec<PlaybackTrackInfo>,
        playback_producer: rtrb::Producer<f32>,
        output_channels: usize,
        is_playing: Arc<AtomicBool>,
    ) -> Result<Self> {
        if tracks.is_empty() {
            return Err(anyhow!("No tracks to play"));
        }

        let mut loaded: Vec<LoadedTrack> = Vec::new();
        let mut max_sample_rate: u32 = 0;

        for info in &tracks {
            let reader = hound::WavReader::open(&info.wav_path)
                .map_err(|e| anyhow!("Failed to open {}: {}", info.wav_path, e))?;
            let spec = reader.spec();
            let wav_ch = spec.channels as usize;
            max_sample_rate = max_sample_rate.max(spec.sample_rate);
            let volume_linear = 10.0f32.powf(info.volume_db / 20.0);

            let raw: Vec<f32> = match spec.sample_format {
                hound::SampleFormat::Float => reader
                    .into_samples::<f32>()
                    .map(|s| s.map_err(|e| anyhow!("{}", e)))
                    .collect::<Result<Vec<f32>>>()?,
                hound::SampleFormat::Int => {
                    let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
                    reader
                        .into_samples::<i32>()
                        .map(|s| s.map(|v| v as f32 / max_val).map_err(|e| anyhow!("{}", e)))
                        .collect::<Result<Vec<f32>>>()?
                }
            };

            let frames = raw.len() / wav_ch;
            let mut normalized = Vec::with_capacity(frames * output_channels);
            for frame_idx in 0..frames {
                for out_ch in 0..output_channels {
                    let sample = if out_ch < wav_ch {
                        raw[frame_idx * wav_ch + out_ch]
                    } else {
                        raw[frame_idx * wav_ch + (wav_ch - 1)]
                    };
                    normalized.push(sample);
                }
            }

            loaded.push(LoadedTrack {
                samples: normalized,
                volume_linear,
            });
        }

        let max_len = loaded.iter().map(|t| t.samples.len()).max().unwrap_or(0);
        let total_frames = if output_channels > 0 {
            max_len as u64 / output_channels as u64
        } else {
            0
        };

        let position_samples = Arc::new(AtomicU64::new(0));
        let position_cb = position_samples.clone();
        let is_playing_cb = is_playing.clone();

        is_playing.store(true, Ordering::SeqCst);

        let handle = std::thread::Builder::new()
            .name("wav-reader".into())
            .spawn(move || -> Result<rtrb::Producer<f32>> {
                mix_tracks_to_ring(
                    playback_producer,
                    loaded,
                    output_channels,
                    &is_playing_cb,
                    &position_cb,
                )
            })?;

        Ok(Self {
            is_playing,
            reader_handle: Some(handle),
            position_samples,
            total_frames,
            sample_rate: max_sample_rate,
        })
    }

    pub fn stop(&mut self) -> Result<rtrb::Producer<f32>> {
        self.is_playing.store(false, Ordering::SeqCst);
        if let Some(handle) = self.reader_handle.take() {
            handle
                .join()
                .map_err(|_| anyhow!("Reader thread panicked"))?
        } else {
            Err(anyhow!("No reader thread"))
        }
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn position_secs(&self) -> f64 {
        self.position_samples.load(Ordering::Relaxed) as f64 / self.sample_rate.max(1) as f64
    }

    #[allow(dead_code)]
    pub fn duration_secs(&self) -> f64 {
        self.total_frames as f64 / self.sample_rate.max(1) as f64
    }
}

fn mix_tracks_to_ring(
    mut producer: rtrb::Producer<f32>,
    tracks: Vec<LoadedTrack>,
    output_channels: usize,
    is_playing: &AtomicBool,
    position: &AtomicU64,
) -> Result<rtrb::Producer<f32>> {
    let max_len = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0);
    let chunk_size = 1024 * output_channels;
    let mut pos: usize = 0;

    while is_playing.load(Ordering::Relaxed) && pos < max_len {
        let end = (pos + chunk_size).min(max_len);
        let len = end - pos;

        let mut mix_buf: Vec<f32> = vec![0.0; len];

        for track in &tracks {
            let track_end = track.samples.len().min(end);
            if pos < track_end {
                for i in pos..track_end {
                    mix_buf[i - pos] += track.samples[i] * track.volume_linear;
                }
            }
        }

        for s in mix_buf.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        let mut written = 0;
        while written < mix_buf.len() && is_playing.load(Ordering::Relaxed) {
            let slots = producer.slots();
            if slots == 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            let to_write = (mix_buf.len() - written).min(slots);
            let chunk = producer.write_chunk_uninit(to_write).unwrap();
            chunk.fill_from_iter(mix_buf[written..written + to_write].iter().copied());
            written += to_write;
        }

        pos = end;
        if output_channels > 0 {
            position.store(pos as u64 / output_channels as u64, Ordering::Relaxed);
        }
    }

    is_playing.store(false, Ordering::SeqCst);
    Ok(producer)
}
