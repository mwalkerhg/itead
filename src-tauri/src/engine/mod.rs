mod devices;
mod dsp;
pub mod params;
mod playback;
mod recording;

pub use devices::{DeviceConfigInfo, DeviceInfo};
pub use params::{AudioEngineConfig, ChannelMode, ChannelStripParams, TonePreset};
pub use playback::PlaybackTrackInfo;
pub use recording::RecordingResult;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use dsp::{AmpSim, BiquadState, Freeverb};
use params::SharedChannelParams;
use playback::PlaybackSession;
use recording::RecordingSession;
use rtrb::RingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

enum EngineCmd {
    StartRecording {
        path: Option<String>,
        reply: mpsc::SyncSender<Result<String>>,
    },
    StopRecording {
        reply: mpsc::SyncSender<Result<Option<RecordingResult>>>,
    },
    StartPlayback {
        tracks: Vec<PlaybackTrackInfo>,
        reply: mpsc::SyncSender<Result<()>>,
    },
    StopPlayback {
        reply: mpsc::SyncSender<Result<()>>,
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
                            reply.send(engine.recording.start(engine.running.clone(), path)).ok();
                        }
                        EngineCmd::StopRecording { reply } => {
                            reply.send(engine.recording.stop()).ok();
                        }
                        EngineCmd::StartPlayback { tracks, reply } => {
                            reply.send(engine.start_playback(tracks)).ok();
                        }
                        EngineCmd::StopPlayback { reply } => {
                            reply.send(engine.stop_playback()).ok();
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

    pub fn stop_recording(&self) -> Result<Option<RecordingResult>> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.cmd_tx
            .send(EngineCmd::StopRecording { reply: reply_tx })
            .map_err(|_| anyhow!("Engine thread not running"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow!("Engine thread died"))?
    }

    pub fn start_playback(&self, tracks: Vec<PlaybackTrackInfo>) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.cmd_tx
            .send(EngineCmd::StartPlayback {
                tracks,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("Engine thread not running"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow!("Engine thread died"))?
    }

    pub fn stop_playback(&self) -> Result<()> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.cmd_tx
            .send(EngineCmd::StopPlayback { reply: reply_tx })
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
        strip.reverb_enabled.store(params.reverb_enabled, Ordering::Relaxed);
        strip.reverb_room_size.store(params.reverb_room_size.to_bits(), Ordering::Relaxed);
        strip.reverb_damping.store(params.reverb_damping.to_bits(), Ordering::Relaxed);
        strip.reverb_wet.store(params.reverb_wet.to_bits(), Ordering::Relaxed);
        strip.tone_preset.store(params.tone_preset.to_u32(), Ordering::Relaxed);
        strip.tone_drive.store(params.tone_drive.to_bits(), Ordering::Relaxed);
    }

    pub fn list_devices() -> Result<(Vec<DeviceInfo>, Vec<DeviceInfo>)> {
        devices::list_devices()
    }
}

struct AudioEngine {
    running: Arc<AtomicBool>,
    recording: RecordingSession,
    playback: Option<PlaybackSession>,
    playback_producer: Option<rtrb::Producer<f32>>,
    is_playing: Arc<AtomicBool>,
    out_channels: usize,
    _input_stream: cpal::Stream,
    _output_stream: cpal::Stream,
}

impl AudioEngine {
    pub fn new(config: AudioEngineConfig, channel_params: Arc<SharedChannelParams>) -> Result<Self> {
        let host = cpal::default_host();

        let input_device = devices::select_device(&host, &config.input_device, true)?;
        let output_device = devices::select_device(&host, &config.output_device, false)?;

        let sample_rate = cpal::SampleRate(config.sample_rate);
        let input_config = devices::find_best_config(&input_device, true, sample_rate, config.buffer_size)?;
        let output_config =
            devices::find_best_config(&output_device, false, sample_rate, config.buffer_size)?;

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

        // --- Playback ring buffer (~1s capacity, output-channel-width) ---
        let playback_capacity = (actual_sample_rate as usize) * out_channels;
        let (playback_producer, mut playback_consumer) = RingBuffer::<f32>::new(playback_capacity);

        let running = Arc::new(AtomicBool::new(true));
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_recording_cb = is_recording.clone();
        let is_playing = Arc::new(AtomicBool::new(false));
        let is_playing_cb = is_playing.clone();

        let params_cb = channel_params.clone();
        let sample_rate_f = actual_sample_rate as f32;
        let mut biquad = [BiquadState::new(), BiquadState::new()];
        let mut reverb = [Freeverb::new(sample_rate_f), Freeverb::new(sample_rate_f)];
        let mut amp_sim = [AmpSim::new(), AmpSim::new()];
        let mut process_buf: Vec<f32> =
            Vec::with_capacity(config.buffer_size as usize * in_channels);

        // --- Input stream ---
        let input_stream = input_device.build_input_stream(
            &input_config,
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                let frame_count = data.len() / in_channels;

                struct StripSnapshot {
                    phase: f32,
                    lowcut_on: bool,
                    lowcut_freq: f32,
                    gain_lin: f32,
                    reverb_on: bool,
                    reverb_room: f32,
                    reverb_damp: f32,
                    reverb_wet: f32,
                    tone_preset: u32,
                    tone_drive: f32,
                }
                let snap = |idx: usize| -> StripSnapshot {
                    let s = &params_cb.strips[idx];
                    StripSnapshot {
                        phase: if s.phase_invert.load(Ordering::Relaxed) { -1.0 } else { 1.0 },
                        lowcut_on: s.lowcut_enabled.load(Ordering::Relaxed),
                        lowcut_freq: s.load_lowcut_freq(),
                        gain_lin: 10.0f32.powf(s.load_gain_db() / 20.0),
                        reverb_on: s.reverb_enabled.load(Ordering::Relaxed),
                        reverb_room: f32::from_bits(s.reverb_room_size.load(Ordering::Relaxed)),
                        reverb_damp: f32::from_bits(s.reverb_damping.load(Ordering::Relaxed)),
                        reverb_wet: f32::from_bits(s.reverb_wet.load(Ordering::Relaxed)),
                        tone_preset: s.tone_preset.load(Ordering::Relaxed),
                        tone_drive: f32::from_bits(s.tone_drive.load(Ordering::Relaxed)),
                    }
                };

                let apply_dsp =
                    |sample: f32, snap: &StripSnapshot, bq: &mut BiquadState, amp: &mut AmpSim| -> f32 {
                        let mut s = sample * snap.phase;
                        if snap.lowcut_on {
                            bq.update_highpass(snap.lowcut_freq, sample_rate_f);
                            s = bq.process(s);
                        }
                        if snap.tone_preset != 0 {
                            amp.configure(snap.tone_preset, sample_rate_f);
                            s = amp.process(s, snap.tone_drive);
                        }
                        s * snap.gain_lin
                    };

                let apply_reverb =
                    |sample: f32, snap: &StripSnapshot, rev: &mut Freeverb| -> f32 {
                        if !snap.reverb_on {
                            return sample;
                        }
                        rev.set_params(snap.reverb_room, snap.reverb_damp);
                        let wet = rev.process(sample);
                        sample * (1.0 - snap.reverb_wet) + wet * snap.reverb_wet
                    };

                process_buf.clear();

                if let Some(ch) = source_channel {
                    let p = snap(ch);
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        let s = apply_dsp(frame[ch], &p, &mut biquad[ch], &mut amp_sim[ch]);
                        process_buf.push(apply_reverb(s, &p, &mut reverb[ch]));
                    }
                } else if merge {
                    let p0 = snap(0);
                    let p1 = snap(1);
                    let inv = 1.0 / in_channels.min(2) as f32;
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        let s0 = apply_dsp(frame[0], &p0, &mut biquad[0], &mut amp_sim[0]);
                        let s0 = apply_reverb(s0, &p0, &mut reverb[0]);
                        let s1 = if in_channels > 1 {
                            let s1 = apply_dsp(frame[1], &p1, &mut biquad[1], &mut amp_sim[1]);
                            apply_reverb(s1, &p1, &mut reverb[1])
                        } else {
                            s0
                        };
                        process_buf.push((s0 + s1) * inv);
                    }
                } else {
                    let p0 = snap(0);
                    let p1 = snap(1);
                    for frame in data.chunks_exact(in_channels).take(frame_count) {
                        let s = apply_dsp(frame[0], &p0, &mut biquad[0], &mut amp_sim[0]);
                        process_buf.push(apply_reverb(s, &p0, &mut reverb[0]));
                        if in_channels > 1 {
                            let s = apply_dsp(frame[1], &p1, &mut biquad[1], &mut amp_sim[1]);
                            process_buf.push(apply_reverb(s, &p1, &mut reverb[1]));
                        }
                        for ch_idx in 2..in_channels {
                            process_buf.push(frame[ch_idx]);
                        }
                    }
                }

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

        // --- Output stream (mixes passthrough + playback) ---
        let output_stream = output_device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                let available = consumer.slots();

                // First: fill from passthrough (monitoring)
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

                // Second: mix in playback audio (already output-channel-width)
                if is_playing_cb.load(Ordering::Relaxed) {
                    let pb_available = playback_consumer.slots();
                    let to_read = data.len().min(pb_available);
                    if to_read > 0 {
                        let chunk = playback_consumer.read_chunk(to_read).unwrap();
                        let (slice_a, slice_b) = chunk.as_slices();
                        let mut i = 0;
                        for &sample in slice_a.iter().chain(slice_b.iter()) {
                            data[i] = (data[i] + sample).clamp(-1.0, 1.0);
                            i += 1;
                        }
                        chunk.commit_all();
                    }
                }
            },
            |err| eprintln!("[ERROR] Output stream error: {}", err),
            None,
        )?;

        output_stream.play()?;
        input_stream.play()?;

        let recording = RecordingSession::new(
            is_recording,
            rec_consumer,
            actual_sample_rate,
            rec_channels,
        );

        Ok(Self {
            running,
            recording,
            playback: None,
            playback_producer: Some(playback_producer),
            is_playing,
            out_channels,
            _input_stream: input_stream,
            _output_stream: output_stream,
        })
    }

    fn start_playback(&mut self, tracks: Vec<PlaybackTrackInfo>) -> Result<()> {
        if let Some(mut pb) = self.playback.take() {
            if let Ok(producer) = pb.stop() {
                self.playback_producer = Some(producer);
            }
        }

        let producer = self
            .playback_producer
            .take()
            .ok_or_else(|| anyhow!("Playback producer not available"))?;

        let session = PlaybackSession::start(
            tracks,
            producer,
            self.out_channels,
            self.is_playing.clone(),
        )?;
        self.playback = Some(session);
        Ok(())
    }

    fn stop_playback(&mut self) -> Result<()> {
        if let Some(mut pb) = self.playback.take() {
            if let Ok(producer) = pb.stop() {
                self.playback_producer = Some(producer);
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<Option<RecordingResult>> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(mut pb) = self.playback.take() {
            pb.stop().ok();
        }
        self.recording.stop()
    }
}
