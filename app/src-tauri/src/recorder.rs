use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

type SharedWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;
pub struct RecordingState(pub Mutex<Option<Recorder>>);
impl Default for RecordingState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

pub struct Recorder {
    pub take_id: String,
    pub project_path: PathBuf,
    pub block_id: String,
    pub relative_path: String,
    started_at: Instant,
    paused_total: Duration,
    paused_at: Option<Instant>,
    active: Arc<AtomicBool>,
    writer: SharedWriter,
    stream: cpal::Stream,
    pub events: Vec<PresentationEvent>,
}

#[derive(Debug, Clone)]
pub struct PresentationEvent {
    pub event_type: String,
    pub project_time_us: i64,
    pub tray_item_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub take_id: String,
    pub block_id: String,
    pub elapsed_us: i64,
    pub paused: bool,
}

pub fn input_devices() -> Result<Vec<String>> {
    let mut names = cpal::default_host()
        .input_devices()?
        .filter_map(|device| {
            device
                .description()
                .ok()
                .map(|description| description.name().to_owned())
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn start(project_path: &str, block_id: &str, device_name: Option<&str>) -> Result<Recorder> {
    let project_path = PathBuf::from(project_path);
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => host
            .input_devices()?
            .find(|device| {
                device
                    .description()
                    .is_ok_and(|description| description.name() == name)
            })
            .with_context(|| format!("Input device is unavailable: {name}"))?,
        None => host
            .default_input_device()
            .context("No microphone is available")?,
    };
    let supported = device.default_input_config()?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let take_id = Uuid::new_v4().to_string();
    let relative_path = format!("recordings/raw/{take_id}.wav");
    let writer = WavWriter::create(
        project_path.join(&relative_path),
        WavSpec {
            channels: 1,
            sample_rate: config.sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    )?;
    let writer = Arc::new(Mutex::new(Some(writer)));
    let active = Arc::new(AtomicBool::new(true));
    let channels = usize::from(config.channels);
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream(
            &device,
            &config,
            writer.clone(),
            active.clone(),
            channels,
            |value: f32| (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16,
        )?,
        cpal::SampleFormat::I16 => build_stream(
            &device,
            &config,
            writer.clone(),
            active.clone(),
            channels,
            |value: i16| value,
        )?,
        cpal::SampleFormat::U16 => build_stream(
            &device,
            &config,
            writer.clone(),
            active.clone(),
            channels,
            |value: u16| (value as i32 - 32_768) as i16,
        )?,
        format => bail!("Unsupported microphone sample format: {format}"),
    };
    stream.play()?;
    Ok(Recorder {
        take_id,
        project_path,
        block_id: block_id.to_owned(),
        relative_path,
        started_at: Instant::now(),
        paused_total: Duration::ZERO,
        paused_at: None,
        active,
        writer,
        stream,
        events: Vec::new(),
    })
}

fn build_stream<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    writer: SharedWriter,
    active: Arc<AtomicBool>,
    channels: usize,
    convert: F,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Copy,
    F: Fn(T) -> i16 + Send + Sync + 'static,
{
    Ok(device.build_input_stream(
        config,
        move |samples: &[T], _| {
            if !active.load(Ordering::Relaxed) {
                return;
            }
            let mut guard = writer.lock();
            let Some(writer) = guard.as_mut() else { return };
            for frame in samples.chunks(channels) {
                let mono = (frame
                    .iter()
                    .map(|sample| i64::from(convert(*sample)))
                    .sum::<i64>()
                    / frame.len() as i64) as i16;
                if let Err(error) = writer.write_sample(mono) {
                    log::error!("failed writing recording sample: {error}");
                    break;
                }
            }
        },
        |error| log::error!("microphone stream error: {error}"),
        None,
    )?)
}

impl Recorder {
    pub fn elapsed_us(&self) -> i64 {
        self.paused_at
            .unwrap_or_else(Instant::now)
            .duration_since(self.started_at)
            .saturating_sub(self.paused_total)
            .as_micros() as i64
    }
    pub fn status(&self) -> RecordingStatus {
        RecordingStatus {
            take_id: self.take_id.clone(),
            block_id: self.block_id.clone(),
            elapsed_us: self.elapsed_us(),
            paused: self.paused_at.is_some(),
        }
    }
    pub fn pause(&mut self) {
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
            self.active.store(false, Ordering::Relaxed);
        }
    }
    pub fn resume(&mut self) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_total += paused_at.elapsed();
            self.active.store(true, Ordering::Relaxed);
        }
    }
    pub fn cue(&mut self, event_type: &str, tray_item_id: Option<String>) {
        self.events.push(PresentationEvent {
            event_type: event_type.to_owned(),
            project_time_us: self.elapsed_us(),
            tray_item_id,
        });
    }
    pub fn finish(mut self) -> Result<FinishedRecording> {
        self.pause();
        self.stream.pause()?;
        let duration_us = self.elapsed_us();
        drop(self.stream);
        self.writer
            .lock()
            .take()
            .context("Recording file is already finalized")?
            .finalize()?;
        Ok(FinishedRecording {
            take_id: self.take_id,
            project_path: self.project_path,
            block_id: self.block_id,
            relative_path: self.relative_path,
            duration_us,
            events: self.events,
        })
    }
}

pub struct FinishedRecording {
    pub take_id: String,
    pub project_path: PathBuf,
    pub block_id: String,
    pub relative_path: String,
    pub duration_us: i64,
    pub events: Vec<PresentationEvent>,
}
