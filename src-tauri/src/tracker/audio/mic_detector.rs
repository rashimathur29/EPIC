use cpal::traits::{DeviceTrait, HostTrait};
use cpal::SampleFormat;

pub struct MicrophoneDetector;

impl MicrophoneDetector {
    pub fn is_microphone_active() -> bool {
        let host = cpal::default_host();

        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                log::warn!("[MIC] No default input device found");
                return false;
            }
        };

        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[MIC] Failed to get default input config: {:?}", e);
                return true; // Conservative: assume in use if can't query
            }
        };

        // Helper to try building stream for any format
        let stream_result = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |_data: &[f32], _: &cpal::InputCallbackInfo| {},
                move |_err| {},
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |_data: &[i16], _: &cpal::InputCallbackInfo| {},
                move |_err| {},
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |_data: &[u16], _: &cpal::InputCallbackInfo| {},
                move |_err| {},
                None,
            ),
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        };

        match stream_result {
            Ok(_stream) => {
                // Successfully opened stream → mic is free
                false
            }
            Err(err) => {
                log::debug!("[MIC] Microphone appears in use: {:?}", err);
                true
            }
        }
    }
}