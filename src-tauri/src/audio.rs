//! Microphone capture.
//!
//! `getUserMedia` is not an option: it needs a secure context and the packaged
//! app serves `http://tauri.localhost`. So capture is cpal, in Rust.

use std::fmt;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
// `Sample` is needed in scope for `f32::from_sample`, not called directly.
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use tauri::{AppHandle, Emitter};

/// ~30 Hz, which is what the waveform is built for.
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

/// Below this the room is quiet, so the bars should sit on the floor rather than
/// twitching at ambient noise.
const NOISE_FLOOR: f32 = 0.004;

#[derive(Debug)]
pub enum AudioError {
    NoDevice,
    Config(String),
    UnsupportedFormat(SampleFormat),
    Build(String),
    Play(String),
    ThreadDied,
}

impl fmt::Display for AudioError {
    /// These strings go straight onto the pill, so they are short and blameless.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(f, "No microphone"),
            Self::UnsupportedFormat(format) => write!(f, "Unsupported mic format ({format:?})"),
            Self::Config(_) | Self::Build(_) | Self::Play(_) | Self::ThreadDied => {
                write!(f, "Microphone unavailable")
            }
        }
    }
}

impl AudioError {
    /// cpal's own wording, for the log. The pill only ever shows `Display`, which
    /// has to stay short enough to fit the pill.
    pub fn detail(&self) -> &str {
        match self {
            Self::Config(detail) | Self::Build(detail) | Self::Play(detail) => detail,
            Self::NoDevice => "no default input device",
            Self::UnsupportedFormat(_) => "sample format not handled",
            Self::ThreadDied => "capture thread exited before reporting",
        }
    }
}

/// A capture in flight. Dropping it stops the stream and closes the device.
pub struct Recorder {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl Recorder {
    pub fn start(app: AppHandle) -> Result<Self, AudioError> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or(AudioError::NoDevice)?;

        let supported = device
            .default_input_config()
            .map_err(|err| AudioError::Config(err.to_string()))?;

        let format = supported.sample_format();
        let channels = supported.channels().max(1) as usize;
        let sample_rate = supported.sample_rate().0;
        let config: StreamConfig = supported.into();

        let samples = Arc::new(Mutex::new(Vec::new()));
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();

        let thread_samples = samples.clone();
        let thread = std::thread::spawn(move || {
            // cpal's `Stream` is `!Send` on WASAPI, so it has to be built, played
            // and dropped on one thread. WASAPI usually hands back F32; the other
            // two are cheap to support and the docs say not to assume.
            let opened = match format {
                SampleFormat::F32 => open::<f32>(&device, &config, channels, thread_samples, app),
                SampleFormat::I16 => open::<i16>(&device, &config, channels, thread_samples, app),
                SampleFormat::U16 => open::<u16>(&device, &config, channels, thread_samples, app),
                other => Err(AudioError::UnsupportedFormat(other)),
            };

            match opened {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    // Park until told to stop. Dropping the stream here, on the
                    // thread that built it, is the whole point of this thread.
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                }
            }
        });

        // Report device failures to the caller synchronously, so the pill can show
        // an error instead of pretending to record.
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop: stop_tx,
                thread: Some(thread),
                samples,
                sample_rate,
            }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(AudioError::ThreadDied),
        }
    }

    /// Mono samples at the device's rate. Resampling to 16 kHz is 2.2's job.
    pub fn stop(mut self) -> (Vec<f32>, u32) {
        let _ = self.stop.send(());

        if let Some(thread) = self.thread.take() {
            // Join before reading: the callback can still be appending until the
            // stream is dropped.
            if thread.join().is_err() {
                eprintln!("piplo: capture thread panicked");
            }
        }

        // A poisoned lock means the callback panicked, but the samples collected
        // before that are still good — and losing them would cost a dictation.
        let mut guard = match self.samples.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        (std::mem::take(&mut *guard), self.sample_rate)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Without this, a Recorder dropped instead of stopped would leave the
        // thread parked forever with the input device open.
        let _ = self.stop.send(());
    }
}

fn open<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    app: AppHandle,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut last_emit = Instant::now();
    let mut sum_squares = 0.0_f64;
    let mut counted = 0_usize;

    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut mono = Vec::with_capacity(data.len() / channels + 1);

                for frame in data.chunks(channels) {
                    let mixed = frame.iter().copied().map(f32::from_sample).sum::<f32>()
                        / frame.len() as f32;
                    mono.push(mixed);
                    sum_squares += (mixed as f64) * (mixed as f64);
                    counted += 1;
                }

                match samples.lock() {
                    Ok(mut buffer) => buffer.extend_from_slice(&mono),
                    Err(poisoned) => poisoned.into_inner().extend_from_slice(&mono),
                }

                // Throttle here, not in JS — this callback fires hundreds of
                // times a second.
                if last_emit.elapsed() >= LEVEL_INTERVAL {
                    let rms = if counted > 0 {
                        (sum_squares / counted as f64).sqrt() as f32
                    } else {
                        0.0
                    };

                    let _ = app.emit("level", meter(rms));

                    sum_squares = 0.0;
                    counted = 0;
                    last_emit = Instant::now();
                }
            },
            |err| eprintln!("piplo: audio stream error: {err}"),
            None,
        )
        .map_err(|err| AudioError::Build(err.to_string()))?;

    stream
        .play()
        .map_err(|err| AudioError::Play(err.to_string()))?;

    Ok(stream)
}

/// Speech RMS sits around 0.02–0.2, so a linear mapping would barely move the
/// bars. The square root opens up the quiet end.
fn meter(rms: f32) -> f32 {
    ((rms - NOISE_FLOOR).max(0.0).sqrt() * 2.4).clamp(0.0, 1.0)
}
