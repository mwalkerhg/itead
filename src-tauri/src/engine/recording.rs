use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingResult {
    pub path: String,
    pub duration_secs: f64,
    pub total_samples: u64,
    pub sample_rate: u32,
    pub channels: u16,
}

pub(crate) struct RecordingSession {
    pub is_recording: Arc<AtomicBool>,
    pub rec_consumer: Option<rtrb::Consumer<f32>>,
    pub writer_handle: Option<std::thread::JoinHandle<Result<u64>>>,
    pub recording_path: Option<String>,
    pub actual_sample_rate: u32,
    pub rec_channels: usize,
}

impl RecordingSession {
    pub fn new(
        is_recording: Arc<AtomicBool>,
        rec_consumer: rtrb::Consumer<f32>,
        actual_sample_rate: u32,
        rec_channels: usize,
    ) -> Self {
        Self {
            is_recording,
            rec_consumer: Some(rec_consumer),
            writer_handle: None,
            recording_path: None,
            actual_sample_rate,
            rec_channels,
        }
    }

    pub fn start(&mut self, running: Arc<AtomicBool>, output_path: Option<String>) -> Result<String> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        let mut rec_consumer = self
            .rec_consumer
            .take()
            .ok_or_else(|| anyhow!("Recording already started (consumer taken)"))?;

        let path = output_path.unwrap_or_else(|| {
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("recording_{}.wav", ts)
        });

        let spec = hound::WavSpec {
            channels: self.rec_channels as u16,
            sample_rate: self.actual_sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let is_recording = self.is_recording.clone();
        let path_clone = path.clone();

        is_recording.store(true, Ordering::SeqCst);

        let handle = std::thread::Builder::new()
            .name("wav-writer".into())
            .spawn(move || -> Result<u64> {
                let mut writer = hound::WavWriter::create(&path_clone, spec)?;
                let mut total_samples: u64 = 0;

                while running.load(Ordering::SeqCst) && is_recording.load(Ordering::SeqCst) {
                    total_samples += drain_to_writer(&mut rec_consumer, &mut writer)?;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                total_samples += drain_to_writer(&mut rec_consumer, &mut writer)?;
                writer.finalize()?;
                Ok(total_samples)
            })?;

        self.writer_handle = Some(handle);
        self.recording_path = Some(path.clone());

        Ok(path)
    }

    pub fn stop(&mut self) -> Result<Option<RecordingResult>> {
        self.is_recording.store(false, Ordering::SeqCst);

        if let Some(handle) = self.writer_handle.take() {
            let path = self.recording_path.take().unwrap_or_default();
            match handle.join() {
                Ok(Ok(total_samples)) => {
                    let total_frames = total_samples / self.rec_channels as u64;
                    let duration_secs = total_frames as f64 / self.actual_sample_rate as f64;
                    Ok(Some(RecordingResult {
                        path,
                        duration_secs,
                        total_samples,
                        sample_rate: self.actual_sample_rate,
                        channels: self.rec_channels as u16,
                    }))
                }
                Ok(Err(e)) => Err(anyhow!("Writer thread failed: {}", e)),
                Err(_) => Err(anyhow!("Writer thread panicked")),
            }
        } else {
            Ok(None)
        }
    }
}

pub(crate) fn drain_to_writer(
    consumer: &mut rtrb::Consumer<f32>,
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
) -> Result<u64> {
    let available = consumer.slots();
    if available == 0 {
        return Ok(0);
    }
    let chunk = consumer.read_chunk(available).unwrap();
    let (slice_a, slice_b) = chunk.as_slices();
    for &sample in slice_a.iter().chain(slice_b.iter()) {
        writer.write_sample(sample)?;
    }
    chunk.commit_all();
    Ok(available as u64)
}
