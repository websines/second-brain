use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};

/// Audio capture mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AudioCaptureMode {
    /// Separate microphone and system audio (ideal for speaker identification)
    Separate,
    /// Combined virtual device (BlackHole Multi-Output) - mic + system mixed together
    Combined,
    /// Microphone only (no system audio capture available)
    MicrophoneOnly,
}

/// Audio capture capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCapabilities {
    pub has_microphone: bool,
    pub has_system_audio: bool,
    pub capture_mode: AudioCaptureMode,
    pub microphone_device: Option<String>,
    pub system_audio_device: Option<String>,
    pub warning_message: Option<String>,
    pub instructions: Option<String>,
}

/// Check available audio capture capabilities
pub fn check_audio_capabilities() -> AudioCapabilities {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();

    // Get default input device (microphone)
    let (has_microphone, microphone_device, is_mic_virtual) = match host.default_input_device() {
        Some(device) => {
            let name = device.name().unwrap_or_default();
            let is_virtual = name.to_lowercase().contains("blackhole")
                || name.to_lowercase().contains("loopback")
                || name.to_lowercase().contains("soundflower")
                || name.to_lowercase().contains("virtual")
                || name.to_lowercase().contains("multi-output");
            (true, Some(name), is_virtual)
        }
        None => (false, None, false),
    };

    // Check for separate system audio loopback device
    let (has_separate_system, system_audio_device) = match host.input_devices() {
        Ok(devices) => {
            let loopback = devices.filter_map(|d| {
                let name = d.name().ok()?;
                // Don't count the same device as both mic and system
                if microphone_device.as_ref().map(|m| m == &name).unwrap_or(false) {
                    return None;
                }
                if name.to_lowercase().contains("blackhole")
                    || name.to_lowercase().contains("loopback")
                    || name.to_lowercase().contains("soundflower")
                    || name.to_lowercase().contains("virtual")
                {
                    Some(name)
                } else {
                    None
                }
            }).next();

            (loopback.is_some(), loopback)
        }
        Err(_) => (false, None),
    };

    // Determine capture mode and messaging
    let (capture_mode, has_system_audio, warning_message, instructions) = if is_mic_virtual {
        // User's default input is a virtual device (likely BlackHole Multi-Output)
        // This means mic + system audio are combined
        (
            AudioCaptureMode::Combined,
            true,  // We do have system audio, but combined with mic
            Some("Your microphone is a virtual device (combined audio). All speakers will be identified via diarization, but we can't automatically distinguish you from others.".to_string()),
            Some("For better speaker identification, consider using a separate microphone device alongside BlackHole for system audio.".to_string()),
        )
    } else if has_separate_system {
        // Ideal setup: separate mic and system audio devices
        (
            AudioCaptureMode::Separate,
            true,
            None,
            None,
        )
    } else {
        // No system audio capture available
        #[cfg(target_os = "macos")]
        let instructions = Some("To capture remote participants' audio on macOS, install BlackHole (https://existential.audio/blackhole/) and set up a Multi-Output Device in Audio MIDI Setup.".to_string());
        #[cfg(target_os = "windows")]
        let instructions: Option<String> = None; // Windows WASAPI loopback should work
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let instructions = Some("System audio capture may require PulseAudio or PipeWire configuration.".to_string());

        (
            AudioCaptureMode::MicrophoneOnly,
            false,
            Some("System audio capture is not available. Remote participants' voices won't be transcribed.".to_string()),
            instructions,
        )
    };

    AudioCapabilities {
        has_microphone,
        has_system_audio,
        capture_mode,
        microphone_device,
        system_audio_device,
        warning_message,
        instructions,
    }
}

/// Audio sample with metadata
#[derive(Debug, Clone)]
pub struct AudioSample {
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub source: AudioSource,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioSource {
    Microphone,     // User's voice
    SystemAudio,    // Meeting participants (Zoom, Teams, etc.)
}

/// Audio capture manager
pub struct AudioCapture {
    is_capturing: Arc<AtomicBool>,
    mic_handle: Option<std::thread::JoinHandle<()>>,
    system_handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            is_capturing: Arc::new(AtomicBool::new(false)),
            mic_handle: None,
            system_handle: None,
        }
    }

    /// Start capturing audio from both microphone and system audio
    pub fn start(&mut self, sender: mpsc::UnboundedSender<AudioSample>) -> Result<(), String> {
        if self.is_capturing.load(Ordering::SeqCst) {
            return Err("Already capturing".to_string());
        }

        self.is_capturing.store(true, Ordering::SeqCst);

        // Start microphone capture
        let mic_sender = sender.clone();
        let mic_capturing = self.is_capturing.clone();
        self.mic_handle = Some(std::thread::spawn(move || {
            if let Err(e) = capture_microphone(mic_sender, mic_capturing) {
                eprintln!("Microphone capture error: {}", e);
            }
        }));

        // Start system audio capture (platform-specific)
        #[cfg(target_os = "macos")]
        {
            let sys_sender = sender;
            let sys_capturing = self.is_capturing.clone();
            self.system_handle = Some(std::thread::spawn(move || {
                if let Err(e) = capture_system_audio_macos(sys_sender, sys_capturing) {
                    eprintln!("System audio capture error: {}", e);
                }
            }));
        }

        #[cfg(target_os = "windows")]
        {
            let sys_sender = sender;
            let sys_capturing = self.is_capturing.clone();
            self.system_handle = Some(std::thread::spawn(move || {
                if let Err(e) = capture_system_audio_windows(sys_sender, sys_capturing) {
                    eprintln!("System audio capture error: {}", e);
                }
            }));
        }

        println!("Audio capture started");
        Ok(())
    }

    /// Stop capturing audio
    pub fn stop(&mut self) {
        self.is_capturing.store(false, Ordering::SeqCst);

        if let Some(handle) = self.mic_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.system_handle.take() {
            let _ = handle.join();
        }

        println!("Audio capture stopped");
    }

    pub fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Capture microphone audio using cpal
fn capture_microphone(
    sender: mpsc::UnboundedSender<AudioSample>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or("No input device available")?;

    let config = device.default_input_config()
        .map_err(|e| e.to_string())?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    println!("Microphone: {} Hz, {} channels", sample_rate, channels);

    let start_time = std::time::Instant::now();

    // Clone for the closure
    let is_capturing_for_callback = is_capturing.clone();

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !is_capturing_for_callback.load(Ordering::SeqCst) {
                return;
            }

            let sample = AudioSample {
                data: data.to_vec(),
                sample_rate,
                channels,
                source: AudioSource::Microphone,
                timestamp_ms: start_time.elapsed().as_millis() as u64,
            };

            let _ = sender.send(sample);
        },
        |err| eprintln!("Microphone stream error: {}", err),
        None,
    ).map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    // Keep thread alive while capturing
    while is_capturing.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

/// Capture system audio on macOS using ScreenCaptureKit
/// Note: This requires macOS 12.3+ and screen recording permissions
#[cfg(target_os = "macos")]
fn capture_system_audio_macos(
    sender: mpsc::UnboundedSender<AudioSample>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String> {
    // For now, we'll use a simplified approach with cpal loopback
    // ScreenCaptureKit requires more complex setup and permissions handling
    // TODO: Implement full ScreenCaptureKit integration

    println!("System audio capture on macOS: Using BlackHole/Loopback if available");

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();

    // Try to find a loopback device (like BlackHole or Loopback)
    let devices = host.input_devices().map_err(|e| e.to_string())?;

    let loopback_device = devices
        .filter_map(|d| {
            let name = d.name().ok()?;
            // Look for virtual audio devices commonly used for system audio capture
            if name.to_lowercase().contains("blackhole")
                || name.to_lowercase().contains("loopback")
                || name.to_lowercase().contains("soundflower")
                || name.to_lowercase().contains("virtual")
            {
                Some(d)
            } else {
                None
            }
        })
        .next();

    let device = match loopback_device {
        Some(d) => {
            println!("Found loopback device: {}", d.name().unwrap_or_default());
            d
        }
        None => {
            println!("No loopback device found. System audio capture unavailable.");
            println!("Install BlackHole (https://existential.audio/blackhole/) for system audio capture.");
            // Keep thread alive but don't capture anything
            while is_capturing.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            return Ok(());
        }
    };

    let config = device.default_input_config()
        .map_err(|e| format!("Failed to get config: {}", e))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    println!("System audio: {} Hz, {} channels", sample_rate, channels);

    let start_time = std::time::Instant::now();
    let is_capturing_for_callback = is_capturing.clone();

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !is_capturing_for_callback.load(Ordering::SeqCst) {
                return;
            }

            let sample = AudioSample {
                data: data.to_vec(),
                sample_rate,
                channels,
                source: AudioSource::SystemAudio,
                timestamp_ms: start_time.elapsed().as_millis() as u64,
            };

            let _ = sender.send(sample);
        },
        |err| eprintln!("System audio stream error: {}", err),
        None,
    ).map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    // Keep thread alive while capturing
    while is_capturing.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

/// Capture system audio using WASAPI loopback (Windows only)
#[cfg(target_os = "windows")]
fn capture_system_audio_windows(
    sender: mpsc::UnboundedSender<AudioSample>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String> {
    use windows::{
        core::*,
        Win32::Media::Audio::*,
        Win32::System::Com::*,
    };

    unsafe {
        // Initialize COM
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .map_err(|e| format!("COM init failed: {}", e))?;

        // Get default audio endpoint for loopback
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ).map_err(|e| format!("Failed to create enumerator: {}", e))?;

        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("Failed to get default device: {}", e))?;

        // Activate audio client
        let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Failed to activate audio client: {}", e))?;

        // Get mix format
        let mix_format = audio_client.GetMixFormat()
            .map_err(|e| format!("Failed to get mix format: {}", e))?;

        let format = &*mix_format;
        let sample_rate = format.nSamplesPerSec;
        let channels = format.nChannels;
        let bits_per_sample = format.wBitsPerSample;

        println!("Windows audio: {} Hz, {} channels, {} bits", sample_rate, channels, bits_per_sample);

        // Initialize for loopback capture
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            10_000_000, // 1 second buffer
            0,
            mix_format,
            None,
        ).map_err(|e| format!("Failed to initialize: {}", e))?;

        // Get capture client
        let capture_client: IAudioCaptureClient = audio_client.GetService()
            .map_err(|e| format!("Failed to get capture client: {}", e))?;

        audio_client.Start()
            .map_err(|e| format!("Failed to start capture: {}", e))?;

        let start_time = std::time::Instant::now();

        while is_capturing.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut packet_length = 0u32;
            capture_client.GetNextPacketSize(&mut packet_length)
                .map_err(|e| format!("GetNextPacketSize failed: {}", e))?;

            while packet_length > 0 {
                let mut data_ptr = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;

                capture_client.GetBuffer(
                    &mut data_ptr,
                    &mut num_frames,
                    &mut flags,
                    None,
                    None,
                ).map_err(|e| format!("GetBuffer failed: {}", e))?;

                if num_frames > 0 && !data_ptr.is_null() {
                    let total_samples = (num_frames as usize) * (channels as usize);

                    // Convert to f32
                    let audio_data: Vec<f32> = if bits_per_sample == 32 {
                        // Already f32
                        let slice = std::slice::from_raw_parts(
                            data_ptr as *const f32,
                            total_samples
                        );
                        slice.to_vec()
                    } else if bits_per_sample == 16 {
                        // Convert from i16
                        let slice = std::slice::from_raw_parts(
                            data_ptr as *const i16,
                            total_samples
                        );
                        slice.iter().map(|&s| s as f32 / 32768.0).collect()
                    } else {
                        vec![]
                    };

                    if !audio_data.is_empty() {
                        let sample = AudioSample {
                            data: audio_data,
                            sample_rate,
                            channels,
                            source: AudioSource::SystemAudio,
                            timestamp_ms: start_time.elapsed().as_millis() as u64,
                        };

                        let _ = sender.send(sample);
                    }
                }

                capture_client.ReleaseBuffer(num_frames)
                    .map_err(|e| format!("ReleaseBuffer failed: {}", e))?;

                capture_client.GetNextPacketSize(&mut packet_length)
                    .map_err(|e| format!("GetNextPacketSize failed: {}", e))?;
            }
        }

        audio_client.Stop()
            .map_err(|e| format!("Failed to stop: {}", e))?;

        CoUninitialize();
    }

    Ok(())
}
