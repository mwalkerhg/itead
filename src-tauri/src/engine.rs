use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use rtrb::RingBuffer;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub configs: Vec<DeviceConfigInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfigInfo {
    pub channels: u16,
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub sample_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelMode {
    Ch1,
    Ch2,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEngineConfig {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub channel_mode: ChannelMode,
    pub merge_to_mono: bool,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            sample_rate: 48000,
            buffer_size: 256,
            channel_mode: ChannelMode::Both,
            merge_to_mono: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingResult {
    pub path: String,
    pub duration_secs: f64,
    pub total_samples: u64,
}

enum EngineCmd {
    StartRecording {
        path: Option<String>,
        reply: mpsc::SyncSender<Result<String>>,
    },
    Stop {
        reply: mpsc::SyncSender<Result<Option<RecordingResult>>>,
    },
}

pub struct EngineHandle {
    cmd_tx: mpsc::Sender<EngineCmd>,
}

impl EngineHandle {
    pub fn start(config: AudioEngineConfig) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<()>>(1);

        std::thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                let mut engine = match AudioEngine::new(config) {
                    Ok(e) => {
                        init_tx.send(Ok(())).ok();
                        e
                    }
                    Err(e) => {
                        init_tx.send(Err(e)).ok();
                        return;
                    }
                };

                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        EngineCmd::StartRecording { path, reply } => {
                            reply.send(engine.start_recording(path)).ok();
                        }
                        EngineCmd::Stop { reply } => {
                            reply.send(engine.stop()).ok();
                            return;
                        }
                    }
                }
            })?;

        init_rx.recv().map_err(|_| anyhow!("Engine thread died during init"))??;
        Ok(Self { cmd_tx })
    }

    pub fn start_recording(&self, path: Option<String>) -> Result<String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.cmd_tx
            .send(EngineCmd::StartRecording {
                path,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Engine thread not running"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow!("Engine thread died"))?
    }

    pub fn stop(self) -> Result<Option<RecordingResult>> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.cmd_tx
            .send(EngineCmd::Stop { reply: reply_tx })
            .map_err(|_| anyhow!("Engine thread not running"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow!("Engine thread died"))?
    }

    pub fn list_devices() -> Result<(Vec<DeviceInfo>, Vec<DeviceInfo>)> {
        AudioEngine::list_devices()
    }
}

struct AudioEngine {
    running: Arc<AtomicBool>,
    is_recording: Arc<AtomicBool>,
    _input_stream: cpal::Stream,
    _output_stream: cpal::Stream,
    rec_consumer: Option<rtrb::Consumer<f32>>,
    writer_handle: Option<std::thread::JoinHandle<Result<u64>>>,
    recording_path: Option<String>,
    actual_sample_rate: u32,
    rec_channels: usize,
}

impl AudioEngine {
    pub fn list_devices() -> Result<(Vec<DeviceInfo>, Vec<DeviceInfo>)> {
        let host = cpal::default_host();

        let default_input_name = host
            .default_input_device()
            .and_then(|d| d.name().ok());
        let default_output_name = host
            .default_output_device()
            .and_then(|d| d.name().ok());

        let inputs = host
            .input_devices()?
            .filter_map(|device| {
                let name = device.name().ok()?;
                let configs = device
                    .supported_input_configs()
                    .ok()?
                    .map(|cfg| DeviceConfigInfo {
                        channels: cfg.channels(),
                        min_sample_rate: cfg.min_sample_rate().0,
                        max_sample_rate: cfg.max_sample_rate().0,
                        sample_format: format!("{:?}", cfg.sample_format()),
                    })
                    .collect();
                Some(DeviceInfo {
                    is_default: default_input_name.as_deref() == Some(&name),
                    name,
                    configs,
                })
            })
            .collect();

        let outputs = host
            .output_devices()?
            .filter_map(|device| {
                let name = device.name().ok()?;
                let configs = device
                    .supported_output_configs()
                    .ok()?
                    .map(|cfg| DeviceConfigInfo {
                        channels: cfg.channels(),
                        min_sample_rate: cfg.min_sample_rate().0,
                        max_sample_rate: cfg.max_sample_rate().0,
                        sample_format: format!("{:?}", cfg.sample_format()),
                    })
                    .collect();
                Some(DeviceInfo {
                    is_default: default_output_name.as_deref() == Some(&name),
                    name,
                    configs,
                })
            })
            .collect();

        Ok((inputs, outputs))
    }

    pub fn new(config: AudioEngineConfig) -> Result<Self> {
        let host = cpal::default_host();

        let input_device = select_device(&host, &config.input_device, true)?;
        let output_device = select_device(&host, &config.output_device, false)?;

        let sample_rate = cpal::SampleRate(config.sample_rate);
        let input_config = find_best_config(&input_device, true, sample_rate, config.buffer_size)?;
        let output_config =
            find_best_config(&output_device, false, sample_rate, config.buffer_size)?;

        let in_channels = input_config.channels as usize;
        let out_channels = output_config.channels as usize;
        let actual_sample_rate = input_config.sample_rate.0;

        let source_channel: Option<usize> = match config.channel_mode {
            ChannelMode::Ch1 => Some(0),
            ChannelMode::Ch2 => Some((1).min(in_channels - 1)),
            ChannelMode::Both => None,
        };
        let merge = source_channel.is_none() && config.merge_to_mono;
        let passthrough_mono = source_channel.is_some() || merge;
        let passthrough_channels = if passthrough_mono { 1 } else { in_channels };
        let rec_channels = if source_channel.is_some() || merge { 1 } else { in_channels };

        // --- Passthrough ring buffer (~0.5s capacity) ---
        let ring_capacity = (actual_sample_rate as usize) * passthrough_channels / 2;
        let (mut passthrough_producer, mut consumer) = RingBuffer::<f32>::new(ring_capacity);

        let prefill = (config.buffer_size as usize) * passthrough_channels;
        {
            let chunk = passthrough_producer.write_chunk_uninit(prefill).unwrap();
            chunk.fill_from_iter(std::iter::repeat(0.0f32).take(prefill));
        }

        // --- Recording ring buffer (always allocated, gated by is_recording) ---
        let rec_capacity = (actual_sample_rate as usize) * rec_channels;
        let (mut rec_producer, rec_consumer) = RingBuffer::<f32>::new(rec_capacity);

        let running = Arc::new(AtomicBool::new(true));
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_recording_cb = is_recording.clone();

        // --- Input stream ---
        let input_stream = input_device.build_input_stream(
            &input_config,
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                let frame_count = data.len() / in_channels;

                if let Some(ch) = source_channel {
                    // Single channel (Ch1 or Ch2): extract one channel for both rings
                    let slots = passthrough_producer.slots();
                    let to_write = frame_count.min(slots);
                    if to_write < frame_count {
                        eprintln!(
                            "[WARN] Passthrough ring overflow — dropped {} frames",
                            frame_count - to_write
                        );
                    }
                    if to_write > 0 {
                        let chunk = passthrough_producer.write_chunk_uninit(to_write).unwrap();
                        chunk.fill_from_iter(
                            data.chunks_exact(in_channels).take(to_write).map(|f| f[ch]),
                        );
                    }

                    if is_recording_cb.load(Ordering::Relaxed) {
                        let slots = rec_producer.slots();
                        let to_write_rec = frame_count.min(slots);
                        if to_write_rec < frame_count {
                            eprintln!(
                                "[WARN] Recording ring overflow — dropped {} frames",
                                frame_count - to_write_rec
                            );
                        }
                        if to_write_rec > 0 {
                            let chunk = rec_producer.write_chunk_uninit(to_write_rec).unwrap();
                            chunk.fill_from_iter(
                                data.chunks_exact(in_channels)
                                    .take(to_write_rec)
                                    .map(|f| f[ch]),
                            );
                        }
                    }
                } else if merge {
                    // Both + merge: mono average for both passthrough and recording
                    let inv = 1.0 / in_channels as f32;

                    let slots = passthrough_producer.slots();
                    let to_write = frame_count.min(slots);
                    if to_write < frame_count {
                        eprintln!(
                            "[WARN] Passthrough ring overflow — dropped {} frames",
                            frame_count - to_write
                        );
                    }
                    if to_write > 0 {
                        let chunk = passthrough_producer.write_chunk_uninit(to_write).unwrap();
                        chunk.fill_from_iter(
                            data.chunks_exact(in_channels)
                                .take(to_write)
                                .map(|f| f.iter().sum::<f32>() * inv),
                        );
                    }

                    if is_recording_cb.load(Ordering::Relaxed) {
                        let slots = rec_producer.slots();
                        let to_write_rec = frame_count.min(slots);
                        if to_write_rec < frame_count {
                            eprintln!(
                                "[WARN] Recording ring overflow — dropped {} frames",
                                frame_count - to_write_rec
                            );
                        }
                        if to_write_rec > 0 {
                            let chunk = rec_producer.write_chunk_uninit(to_write_rec).unwrap();
                            chunk.fill_from_iter(
                                data.chunks_exact(in_channels)
                                    .take(to_write_rec)
                                    .map(|f| f.iter().sum::<f32>() * inv),
                            );
                        }
                    }
                } else {
                    // Both stereo: same data to both rings
                    let to_write = data.len().min(passthrough_producer.slots());
                    if to_write < data.len() {
                        eprintln!(
                            "[WARN] Passthrough ring overflow — dropped {} samples",
                            data.len() - to_write
                        );
                    }
                    if to_write > 0 {
                        let chunk = passthrough_producer.write_chunk_uninit(to_write).unwrap();
                        chunk.fill_from_iter(data[..to_write].iter().copied());
                    }

                    if is_recording_cb.load(Ordering::Relaxed) {
                        let to_write_rec = data.len().min(rec_producer.slots());
                        if to_write_rec < data.len() {
                            eprintln!(
                                "[WARN] Recording ring overflow — dropped {} samples",
                                data.len() - to_write_rec
                            );
                        }
                        if to_write_rec > 0 {
                            let chunk = rec_producer.write_chunk_uninit(to_write_rec).unwrap();
                            chunk.fill_from_iter(data[..to_write_rec].iter().copied());
                        }
                    }
                }
            },
            |err| eprintln!("[ERROR] Input stream error: {}", err),
            None,
        )?;

        // --- Output stream ---
        let output_stream = output_device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                let available = consumer.slots();

                if passthrough_mono {
                    let frames_needed = data.len() / out_channels;
                    let to_read = frames_needed.min(available);
                    if to_read > 0 {
                        let chunk = consumer.read_chunk(to_read).unwrap();
                        let (slice_a, slice_b) = chunk.as_slices();
                        let mut i = 0;
                        for &sample in slice_a.iter().chain(slice_b.iter()) {
                            for _ in 0..out_channels {
                                data[i] = sample;
                                i += 1;
                            }
                        }
                        chunk.commit_all();
                    }
                    for sample in &mut data[to_read * out_channels..] {
                        *sample = 0.0;
                    }
                } else if in_channels == out_channels {
                    let to_read = data.len().min(available);
                    if to_read > 0 {
                        let chunk = consumer.read_chunk(to_read).unwrap();
                        let (slice_a, slice_b) = chunk.as_slices();
                        let a_len = slice_a.len();
                        data[..a_len].copy_from_slice(slice_a);
                        data[a_len..to_read].copy_from_slice(slice_b);
                        chunk.commit_all();
                    }
                    for sample in &mut data[to_read..] {
                        *sample = 0.0;
                    }
                } else {
                    let frames_needed = data.len() / out_channels;
                    let in_samples_needed = frames_needed * in_channels;
                    let to_read = in_samples_needed.min(available);
                    let frames_available = to_read / in_channels;

                    if frames_available > 0 {
                        let chunk =
                            consumer.read_chunk(frames_available * in_channels).unwrap();
                        let (slice_a, slice_b) = chunk.as_slices();

                        let mut in_buf: Vec<f32> =
                            Vec::with_capacity(frames_available * in_channels);
                        in_buf.extend_from_slice(slice_a);
                        in_buf.extend_from_slice(slice_b);
                        chunk.commit_all();

                        for frame in 0..frames_available {
                            for out_ch in 0..out_channels {
                                let out_idx = frame * out_channels + out_ch;
                                if out_ch < in_channels {
                                    let in_idx = frame * in_channels + out_ch;
                                    data[out_idx] = in_buf[in_idx];
                                } else {
                                    data[out_idx] = 0.0;
                                }
                            }
                        }
                    }

                    let filled = frames_available * out_channels;
                    for sample in &mut data[filled..] {
                        *sample = 0.0;
                    }
                }
            },
            |err| eprintln!("[ERROR] Output stream error: {}", err),
            None,
        )?;

        // Start output first so consumer is ready
        output_stream.play()?;
        input_stream.play()?;

        Ok(Self {
            running,
            is_recording,
            _input_stream: input_stream,
            _output_stream: output_stream,
            rec_consumer: Some(rec_consumer),
            writer_handle: None,
            recording_path: None,
            actual_sample_rate,
            rec_channels,
        })
    }

    pub fn start_recording(&mut self, output_path: Option<String>) -> Result<String> {
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

        let running = self.running.clone();
        let is_recording = self.is_recording.clone();
        let path_clone = path.clone();

        // Signal that recording is active before spawning the writer
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

                // Final drain
                total_samples += drain_to_writer(&mut rec_consumer, &mut writer)?;
                writer.finalize()?;
                Ok(total_samples)
            })?;

        self.writer_handle = Some(handle);
        self.recording_path = Some(path.clone());

        Ok(path)
    }

    fn stop(&mut self) -> Result<Option<RecordingResult>> {
        self.running.store(false, Ordering::SeqCst);
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

// ─── Helper functions ───────────────────────────────────────────────

fn drain_to_writer(
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

fn select_device(
    host: &cpal::Host,
    name_filter: &Option<String>,
    is_input: bool,
) -> Result<Device> {
    if let Some(filter) = name_filter {
        let filter_lower = filter.to_lowercase();
        let devices: Box<dyn Iterator<Item = Device>> = if is_input {
            Box::new(host.input_devices()?)
        } else {
            Box::new(host.output_devices()?)
        };

        for device in devices {
            if let Ok(name) = device.name() {
                if name.to_lowercase().contains(&filter_lower) {
                    return Ok(device);
                }
            }
        }
        return Err(anyhow!(
            "No {} device matching '{}' found",
            if is_input { "input" } else { "output" },
            filter
        ));
    }

    if is_input {
        host.default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))
    } else {
        host.default_output_device()
            .ok_or_else(|| anyhow!("No default output device found"))
    }
}

fn find_best_config(
    device: &Device,
    is_input: bool,
    desired_rate: cpal::SampleRate,
    buffer_size: u32,
) -> Result<StreamConfig> {
    let supported: Vec<_> = if is_input {
        device.supported_input_configs()?.collect()
    } else {
        device.supported_output_configs()?.collect()
    };

    let format_priority = [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16];

    for format in &format_priority {
        for cfg in &supported {
            if cfg.sample_format() == *format
                && cfg.min_sample_rate() <= desired_rate
                && cfg.max_sample_rate() >= desired_rate
            {
                let mut config: StreamConfig = cfg.with_sample_rate(desired_rate).into();
                config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
                return Ok(config);
            }
        }
    }

    if let Some(cfg) = supported.first() {
        let rate = cfg.max_sample_rate();
        eprintln!(
            "[WARN] Desired sample rate {} not supported, falling back to {} Hz",
            desired_rate.0, rate.0
        );
        let mut config: StreamConfig = cfg.with_sample_rate(rate).into();
        config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
        return Ok(config);
    }

    Err(anyhow!("No supported audio configs found for device"))
}
