use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use rtrb::RingBuffer;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
pub struct ChannelStripParams {
    pub gain_db: f32,
    pub lowcut_enabled: bool,
    pub lowcut_freq_hz: f32,
    pub phase_invert: bool,
}

impl Default for ChannelStripParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            lowcut_enabled: false,
            lowcut_freq_hz: 80.0,
            phase_invert: false,
        }
    }
}

struct AtomicChannelStrip {
    gain_db: AtomicU32,
    lowcut_enabled: AtomicBool,
    lowcut_freq_hz: AtomicU32,
    phase_invert: AtomicBool,
}

impl AtomicChannelStrip {
    fn new() -> Self {
        Self {
            gain_db: AtomicU32::new(0.0f32.to_bits()),
            lowcut_enabled: AtomicBool::new(false),
            lowcut_freq_hz: AtomicU32::new(80.0f32.to_bits()),
            phase_invert: AtomicBool::new(false),
        }
    }

    fn load_gain_db(&self) -> f32 {
        f32::from_bits(self.gain_db.load(Ordering::Relaxed))
    }

    fn load_lowcut_freq(&self) -> f32 {
        f32::from_bits(self.lowcut_freq_hz.load(Ordering::Relaxed))
    }
}

pub struct SharedChannelParams {
    strips: [AtomicChannelStrip; 2],
}

impl SharedChannelParams {
    pub fn new() -> Self {
        Self {
            strips: [AtomicChannelStrip::new(), AtomicChannelStrip::new()],
        }
    }
}

struct BiquadState {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    last_freq: f32,
    last_sample_rate: f32,
}

impl BiquadState {
    fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            last_freq: 0.0,
            last_sample_rate: 0.0,
        }
    }

    fn update_highpass(&mut self, freq_hz: f32, sample_rate: f32) {
        if freq_hz == self.last_freq && sample_rate == self.last_sample_rate {
            return;
        }
        self.last_freq = freq_hz;
        self.last_sample_rate = sample_rate;

        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 + cos_w0) / 2.0) / a0;
        self.b1 = (-(1.0 + cos_w0)) / a0;
        self.b2 = ((1.0 + cos_w0) / 2.0) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
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
    channel_params: Arc<SharedChannelParams>,
}

impl EngineHandle {
    pub fn start(config: AudioEngineConfig) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<()>>(1);

        let channel_params = Arc::new(SharedChannelParams::new());
        let params_for_engine = channel_params.clone();

        std::thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                let mut engine = match AudioEngine::new(config, params_for_engine) {
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
        Ok(Self { cmd_tx, channel_params })
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

    pub fn update_channel_params(&self, channel: u8, params: &ChannelStripParams) {
        let idx = (channel as usize).min(1);
        let strip = &self.channel_params.strips[idx];
        strip.gain_db.store(params.gain_db.to_bits(), Ordering::Relaxed);
        strip.lowcut_enabled.store(params.lowcut_enabled, Ordering::Relaxed);
        strip.lowcut_freq_hz.store(params.lowcut_freq_hz.to_bits(), Ordering::Relaxed);
        strip.phase_invert.store(params.phase_invert, Ordering::Relaxed);
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

    pub fn new(config: AudioEngineConfig, channel_params: Arc<SharedChannelParams>) -> Result<Self> {
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

        let params_cb = channel_params.clone();
        let sample_rate_f = actual_sample_rate as f32;
        let mut biquad = [BiquadState::new(), BiquadState::new()];
        let mut process_buf: Vec<f32> =
            Vec::with_capacity(config.buffer_size as usize * in_channels);

        // --- Input stream ---
        let input_stream = input_device.build_input_stream(
            &input_config,
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                let frame_count = data.len() / in_channels;

                // Load per-channel params once per buffer
                struct StripSnapshot {
                    phase: f32,
                    lowcut_on: bool,
                    lowcut_freq: f32,
                    gain_lin: f32,
                }
                let snap = |idx: usize| -> StripSnapshot {
                    let s = &params_cb.strips[idx];
                    StripSnapshot {
                        phase: if s.phase_invert.load(Ordering::Relaxed) { -1.0 } else { 1.0 },
                        lowcut_on: s.lowcut_enabled.load(Ordering::Relaxed),
                        lowcut_freq: s.load_lowcut_freq(),
                        gain_lin: 10.0f32.powf(s.load_gain_db() / 20.0),
                    }
                };

                let apply_dsp =
                    |sample: f32, snap: &StripSnapshot, bq: &mut BiquadState| -> f32 {
                        let mut s = sample * snap.phase;
                        if snap.lowcut_on {
                            bq.update_highpass(snap.lowcut_freq, sample_rate_f);
                            s = bq.process(s);
                        }
                        s * snap.gain_lin
                    };

                // Extract + process into pre-allocated buffer
                process_buf.clear();

                if let Some(ch) = source_channel {
                    let p = snap(ch);
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        process_buf.push(apply_dsp(frame[ch], &p, &mut biquad[ch]));
                    }
                } else if merge {
                    let p0 = snap(0);
                    let p1 = snap(1);
                    let inv = 1.0 / in_channels.min(2) as f32;
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        let s0 = apply_dsp(frame[0], &p0, &mut biquad[0]);
                        let s1 = if in_channels > 1 {
                            apply_dsp(frame[1], &p1, &mut biquad[1])
                        } else {
                            s0
                        };
                        process_buf.push((s0 + s1) * inv);
                    }
                } else {
                    let p0 = snap(0);
                    let p1 = snap(1);
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        process_buf.push(apply_dsp(frame[0], &p0, &mut biquad[0]));
                        if in_channels > 1 {
                            process_buf.push(apply_dsp(frame[1], &p1, &mut biquad[1]));
                        }
                        for ch_idx in 2..in_channels {
                            process_buf.push(frame[ch_idx]);
                        }
                    }
                }

                // Write processed buffer to passthrough ring
                let buf_len = process_buf.len();
                let slots = passthrough_producer.slots();
                let to_write = buf_len.min(slots);
                if to_write < buf_len {
                    eprintln!(
                        "[WARN] Passthrough ring overflow — dropped {} samples",
                        buf_len - to_write
                    );
                }
                if to_write > 0 {
                    let chunk = passthrough_producer.write_chunk_uninit(to_write).unwrap();
                    chunk.fill_from_iter(process_buf[..to_write].iter().copied());
                }

                // Write processed buffer to recording ring
                if is_recording_cb.load(Ordering::Relaxed) {
                    let rec_slots = rec_producer.slots();
                    let to_write_rec = buf_len.min(rec_slots);
                    if to_write_rec < buf_len {
                        eprintln!(
                            "[WARN] Recording ring overflow — dropped {} samples",
                            buf_len - to_write_rec
                        );
                    }
                    if to_write_rec > 0 {
                        let chunk = rec_producer.write_chunk_uninit(to_write_rec).unwrap();
                        chunk.fill_from_iter(process_buf[..to_write_rec].iter().copied());
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
