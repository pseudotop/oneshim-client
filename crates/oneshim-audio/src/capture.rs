//! Cross-platform microphone capture with automatic resampling to 16kHz mono.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use parking_lot::Mutex;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use tracing::{debug, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::audio::AudioBuffer;

const TARGET_SAMPLE_RATE: u32 = 16000;

pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
    capturing: Arc<AtomicBool>,
    stream: Mutex<Option<cpal::Stream>>,
    /// Native sample rate of the input device (set during start).
    native_rate: Mutex<Option<u32>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            capturing: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
            native_rate: Mutex::new(None),
        }
    }

    /// Start capturing audio from the default input device.
    pub fn start(&self) -> Result<(), CoreError> {
        if self.capturing.load(Ordering::SeqCst) {
            return Err(CoreError::AudioCapture("already capturing".into()));
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| CoreError::AudioCapture("no input device available".into()))?;

        let config = device
            .default_input_config()
            .map_err(|e| CoreError::AudioCapture(format!("input config error: {e}")))?;

        let native_rate = config.sample_rate();
        let channels = config.channels() as usize;

        debug!(
            device = ?device.name().unwrap_or_default(),
            sample_rate = native_rate,
            channels,
            "starting audio capture"
        );

        // Store native rate for resampling in stop()
        *self.native_rate.lock() = Some(native_rate);

        // Clear buffer for new recording
        self.buffer.lock().clear();
        self.capturing.store(true, Ordering::SeqCst);

        let buffer = self.buffer.clone();
        let capturing = self.capturing.clone();

        let err_fn = |err: cpal::StreamError| {
            warn!("audio stream error: {err}");
        };

        // Callback: downmix to mono and accumulate raw samples (no resampling).
        // Resampling is done in stop() over the full buffer — avoids
        // SincFixedIn chunk-size constraint in variable-size callbacks.
        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !capturing.load(Ordering::SeqCst) {
                        return;
                    }
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    buffer.lock().extend_from_slice(&mono);
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => {
                let buffer = self.buffer.clone();
                let capturing = self.capturing.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !capturing.load(Ordering::SeqCst) {
                            return;
                        }
                        let mono: Vec<f32> = data
                            .chunks(channels)
                            .map(|frame| {
                                frame.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>()
                                    / channels as f32
                            })
                            .collect();
                        buffer.lock().extend_from_slice(&mono);
                    },
                    err_fn,
                    None,
                )
            }
            format => {
                return Err(CoreError::AudioCapture(format!(
                    "unsupported sample format: {format:?}"
                )));
            }
        }
        .map_err(|e| CoreError::AudioCapture(format!("build stream: {e}")))?;

        stream
            .play()
            .map_err(|e| CoreError::AudioCapture(format!("play stream: {e}")))?;

        *self.stream.lock() = Some(stream);
        Ok(())
    }

    /// Stop capturing and return the accumulated audio buffer resampled to 16kHz.
    pub fn stop(&self) -> Result<AudioBuffer, CoreError> {
        self.capturing.store(false, Ordering::SeqCst);

        // Drop the stream to release the device
        if let Some(stream) = self.stream.lock().take() {
            drop(stream);
        }

        let raw_samples: Vec<f32> = std::mem::take(&mut *self.buffer.lock());
        let native_rate = (*self.native_rate.lock()).unwrap_or(TARGET_SAMPLE_RATE);

        debug!(samples = raw_samples.len(), native_rate, "audio capture stopped");

        if raw_samples.is_empty() {
            return Ok(AudioBuffer::new(Vec::new()));
        }

        // Resample to 16kHz if needed
        let samples_16k = if native_rate != TARGET_SAMPLE_RATE {
            resample(&raw_samples, native_rate, TARGET_SAMPLE_RATE)?
        } else {
            raw_samples
        };

        Ok(AudioBuffer::new(samples_16k))
    }

    /// Whether currently capturing.
    pub fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }
}

impl oneshim_core::ports::audio_capture::AudioCapturePort for AudioCapture {
    fn start(&self) -> Result<(), CoreError> {
        AudioCapture::start(self)
    }

    fn stop(&self) -> Result<AudioBuffer, CoreError> {
        AudioCapture::stop(self)
    }

    fn is_capturing(&self) -> bool {
        AudioCapture::is_capturing(self)
    }
}

/// Resample mono f32 audio from `from_rate` to `to_rate` using rubato SincFixedIn.
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, CoreError> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk_size = 1024;
    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        1.0,
        params,
        chunk_size,
        1, // mono
    )
    .map_err(|e| CoreError::AudioCapture(format!("resampler init: {e}")))?;

    let mut output = Vec::with_capacity(
        (input.len() as f64 * to_rate as f64 / from_rate as f64) as usize + chunk_size,
    );

    // Process in chunks of chunk_size
    let mut pos = 0;
    while pos + chunk_size <= input.len() {
        let chunk = input[pos..pos + chunk_size].to_vec();
        let resampled = resampler
            .process(&[chunk], None)
            .map_err(|e| CoreError::AudioCapture(format!("resample: {e}")))?;
        output.extend_from_slice(&resampled[0]);
        pos += chunk_size;
    }

    // Handle remaining samples by zero-padding to chunk_size
    if pos < input.len() {
        let mut last_chunk = vec![0.0f32; chunk_size];
        let remaining = input.len() - pos;
        last_chunk[..remaining].copy_from_slice(&input[pos..]);
        let resampled = resampler
            .process(&[last_chunk], None)
            .map_err(|e| CoreError::AudioCapture(format!("resample tail: {e}")))?;
        let expected = (remaining as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
        let take = expected.min(resampled[0].len());
        output.extend_from_slice(&resampled[0][..take]);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_capture_not_capturing() {
        let capture = AudioCapture::new();
        assert!(!capture.is_capturing());
    }

    #[test]
    fn stop_without_start_returns_empty_buffer() {
        let capture = AudioCapture::new();
        let buffer = capture.stop().unwrap();
        assert!(buffer.is_empty());
        assert_eq!(buffer.sample_rate, 16000);
    }

    #[test]
    fn resample_48k_to_16k() {
        // 48000 Hz sine wave → 16000 Hz: output should be ~1/3 the length
        let input: Vec<f32> = (0..4800)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48000.0).sin())
            .collect();
        let output = resample(&input, 48000, 16000).unwrap();
        // Expected ~1600 samples (4800 * 16000/48000)
        assert!((output.len() as i32 - 1600).unsigned_abs() < 50);
    }
}
