use anyhow::{anyhow, Result};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use rtrb::RingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

/// ITEAD — Input Transform Effects Audio Desktop
#[derive(Parser, Debug)]
#[command(name = "itead")]
struct Args {
    /// List all available audio devices and exit
    #[arg(long)]
    list_devices: bool,

    /// Input device name (substring match). Defaults to system default.
    #[arg(long)]
    input: Option<String>,

    /// Output device name (substring match). Defaults to system default.
    #[arg(long)]
    output: Option<String>,

    /// Desired sample rate in Hz (e.g. 44100, 48000, 96000)
    #[arg(long, default_value_t = 48000)]
    sample_rate: u32,

    /// Buffer size in frames (lower = less latency, more CPU risk)
    #[arg(long, default_value_t = 256)]
    buffer_size: u32,

    /// Enable recording input to a WAV file
    #[arg(long)]
    record: bool,

    /// Output WAV file path (default: recording_<timestamp>.wav)
    #[arg(long)]
    output_file: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let host = cpal::default_host();

    if args.list_devices {
        list_all_devices(&host)?;
        return Ok(());
    }

    // --- Select devices ---
    let input_device = select_device(&host, &args.input, true)?;
    let output_device = select_device(&host, &args.output, false)?;

    println!("Input:  {}", input_device.name()?);
    println!("Output: {}", output_device.name()?);

    // --- Query supported configs ---
    print_supported_configs(&input_device, true)?;
    print_supported_configs(&output_device, false)?;

    // --- Build matching stream configs ---
    let sample_rate = cpal::SampleRate(args.sample_rate);

    let input_config = find_best_config(&input_device, true, sample_rate, args.buffer_size)?;
    let output_config = find_best_config(&output_device, false, sample_rate, args.buffer_size)?;

    let in_channels = input_config.channels as usize;
    let out_channels = output_config.channels as usize;

    println!("\nStream config:");
    println!(
        "  Input:  {} ch, {} Hz, buffer {}",
        in_channels, input_config.sample_rate.0, args.buffer_size
    );
    println!(
        "  Output: {} ch, {} Hz, buffer {}",
        out_channels, output_config.sample_rate.0, args.buffer_size
    );

    // --- Ring buffer for passthrough ---
    let ring_capacity = (args.buffer_size as usize) * in_channels * 4;
    let (mut passthrough_producer, mut consumer) = RingBuffer::<f32>::new(ring_capacity);

    // --- Recording ring buffer (only if recording) ---
    let (mut rec_producer, rec_consumer) = if args.record {
        let rec_capacity = (args.buffer_size as usize) * in_channels * 8;
        let (prod, cons) = RingBuffer::<f32>::new(rec_capacity);
        (Some(prod), Some(cons))
    } else {
        (None, None)
    };

    // --- Shutdown signal ---
    let running = Arc::new(AtomicBool::new(true));
    let running_ctrlc = running.clone();

    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        running_ctrlc.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // --- Build input stream ---
    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            // Push to passthrough ring buffer
            let to_write = data.len().min(passthrough_producer.slots());
            if to_write < data.len() {
                eprintln!(
                    "[WARN] Passthrough ring buffer overflow — dropped {} samples",
                    data.len() - to_write
                );
            }
            if to_write > 0 {
                let chunk = passthrough_producer.write_chunk_uninit(to_write).unwrap();
                chunk.fill_from_iter(data[..to_write].iter().copied());
            }

            // Push to recording ring buffer
            if let Some(ref mut rec_prod) = rec_producer {
                let to_write_rec = data.len().min(rec_prod.slots());
                if to_write_rec < data.len() {
                    eprintln!(
                        "[WARN] Recording ring buffer overflow — dropped {} samples",
                        data.len() - to_write_rec
                    );
                }
                if to_write_rec > 0 {
                    let chunk = rec_prod.write_chunk_uninit(to_write_rec).unwrap();
                    chunk.fill_from_iter(data[..to_write_rec].iter().copied());
                }
            }
        },
        |err| eprintln!("[ERROR] Input stream error: {}", err),
        None,
    )?;

    // --- Build output stream ---
    let output_stream = output_device.build_output_stream(
        &output_config,
        move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
            let available = consumer.slots();

            if in_channels == out_channels {
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
                    let chunk = consumer.read_chunk(frames_available * in_channels).unwrap();
                    let (slice_a, slice_b) = chunk.as_slices();

                    let mut in_buf: Vec<f32> = Vec::with_capacity(frames_available * in_channels);
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

    // --- Spawn recording writer thread ---
    let writer_handle = if let Some(mut rec_consumer) = rec_consumer {
        let output_path = args.output_file.unwrap_or_else(|| {
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("recording_{}.wav", ts)
        });

        let spec = hound::WavSpec {
            channels: in_channels as u16,
            sample_rate: args.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let running_writer = running.clone();
        let path_display = output_path.clone();

        let handle = std::thread::Builder::new()
            .name("wav-writer".into())
            .spawn(move || -> Result<u64> {
                let mut writer = hound::WavWriter::create(&output_path, spec)?;
                let mut total_samples: u64 = 0;

                while running_writer.load(Ordering::SeqCst) {
                    total_samples += drain_to_writer(&mut rec_consumer, &mut writer)?;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                // Final drain after shutdown signal
                total_samples += drain_to_writer(&mut rec_consumer, &mut writer)?;
                writer.finalize()?;
                Ok(total_samples)
            })?;

        println!("Recording to: {}", path_display);
        Some((handle, path_display))
    } else {
        None
    };

    // --- Start streaming ---
    input_stream.play()?;
    output_stream.play()?;

    println!("\n=== PASSTHROUGH ACTIVE ===");
    if args.record {
        println!("Recording is ON.");
    }
    println!("Press Ctrl+C to stop.\n");

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Stop streams first so no more samples arrive
    drop(input_stream);
    drop(output_stream);

    // Wait for writer thread to finish flushing
    if let Some((handle, path)) = writer_handle {
        match handle.join() {
            Ok(Ok(total_samples)) => {
                let total_frames = total_samples / in_channels as u64;
                let duration_secs = total_frames as f64 / args.sample_rate as f64;
                println!(
                    "Saved {} ({:.1}s, {} samples)",
                    path, duration_secs, total_samples
                );
            }
            Ok(Err(e)) => eprintln!("[ERROR] Writer thread failed: {}", e),
            Err(_) => eprintln!("[ERROR] Writer thread panicked"),
        }
    }

    println!("Done.");
    Ok(())
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

/// List all audio devices with their supported configurations
fn list_all_devices(host: &cpal::Host) -> Result<()> {
    println!("=== Available Audio Devices ===\n");

    println!("--- Input Devices ---");
    for device in host.input_devices()? {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let configs = device
            .supported_input_configs()
            .map(|cfgs| cfgs.collect::<Vec<_>>())
            .unwrap_or_default();

        println!("  {}", name);
        for cfg in &configs {
            println!(
                "    {} ch | {}-{} Hz | {:?}",
                cfg.channels(),
                cfg.min_sample_rate().0,
                cfg.max_sample_rate().0,
                cfg.sample_format(),
            );
        }
    }

    println!("\n--- Output Devices ---");
    for device in host.output_devices()? {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let configs = device
            .supported_output_configs()
            .map(|cfgs| cfgs.collect::<Vec<_>>())
            .unwrap_or_default();

        println!("  {}", name);
        for cfg in &configs {
            println!(
                "    {} ch | {}-{} Hz | {:?}",
                cfg.channels(),
                cfg.min_sample_rate().0,
                cfg.max_sample_rate().0,
                cfg.sample_format(),
            );
        }
    }

    if let Some(d) = host.default_input_device() {
        println!("\nDefault input:  {}", d.name()?);
    }
    if let Some(d) = host.default_output_device() {
        println!("Default output: {}", d.name()?);
    }

    Ok(())
}

/// Find a device by substring match, or fall back to the system default
fn select_device(host: &cpal::Host, name_filter: &Option<String>, is_input: bool) -> Result<Device> {
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
            "No {} device matching '{}' found. Run with --list-devices to see available devices.",
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

/// Print supported configurations for a device
fn print_supported_configs(device: &Device, is_input: bool) -> Result<()> {
    let label = if is_input { "Input" } else { "Output" };
    let configs: Vec<_> = if is_input {
        device.supported_input_configs()?.collect()
    } else {
        device.supported_output_configs()?.collect()
    };

    println!("\n{} device supported configs:", label);
    for cfg in &configs {
        println!(
            "  {} ch | {}-{} Hz | {:?}",
            cfg.channels(),
            cfg.min_sample_rate().0,
            cfg.max_sample_rate().0,
            cfg.sample_format(),
        );
    }
    Ok(())
}

/// Find the best matching config for our desired sample rate and buffer size.
/// Prefers F32 sample format for simplicity (no conversion needed).
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
        println!(
            "[WARN] Desired sample rate {} not supported, falling back to {} Hz",
            desired_rate.0, rate.0
        );
        let mut config: StreamConfig = cfg.with_sample_rate(rate).into();
        config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
        return Ok(config);
    }

    Err(anyhow!("No supported audio configs found for device"))
}
