use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use serde::{Deserialize, Serialize};

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

pub fn select_device(
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

pub fn find_best_config(
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
