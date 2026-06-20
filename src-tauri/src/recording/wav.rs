use super::{
    error::{recording_error, RecordingError, RecordingErrorCode},
    sample::sanitize_sample,
    types::{
        OUTPUT_CHANNELS, TARGET_WAV_SAMPLE_RATE, WAV_AUDIO_FORMAT_PCM, WAV_BITS_PER_SAMPLE,
        WAV_HEADER_LEN,
    },
};

fn apply_agc(samples: &mut [f32]) {
    const TARGET_RMS: f32 = 0.158;
    const MAX_GAIN: f32 = 10.0;
    const NOISE_FLOOR: f32 = 0.001;

    if samples.is_empty() {
        return;
    }

    let sum_sq: f64 = samples.iter().map(|s| (*s as f64).powi(2)).sum();
    let rms = ((sum_sq / samples.len() as f64).sqrt()) as f32;

    if rms <= NOISE_FLOOR || !rms.is_finite() {
        return;
    }

    let gain = (TARGET_RMS / rms).min(MAX_GAIN);

    for sample in samples.iter_mut() {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

pub fn encode_recording_wav(
    samples: &[f32],
    input_sample_rate: u32,
) -> Result<Vec<u8>, RecordingError> {
    let mut output_samples =
        resample_mono_linear(samples, input_sample_rate, TARGET_WAV_SAMPLE_RATE)?;
    apply_agc(&mut output_samples);
    encode_pcm16_wav(&output_samples, TARGET_WAV_SAMPLE_RATE, OUTPUT_CHANNELS)
}

pub fn resample_mono_linear(
    samples: &[f32],
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Result<Vec<f32>, RecordingError> {
    if input_sample_rate == 0 || output_sample_rate == 0 {
        return Err(recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        ));
    }

    if samples.is_empty() {
        return Ok(Vec::new());
    }

    if input_sample_rate == output_sample_rate {
        return Ok(samples
            .iter()
            .map(|sample| sanitize_sample(*sample))
            .collect());
    }

    let output_len = ((samples.len() as f64) * (output_sample_rate as f64)
        / (input_sample_rate as f64))
        .round()
        .max(1.0) as usize;
    let mut output = Vec::with_capacity(output_len);

    for index in 0..output_len {
        let source_position =
            (index as f64) * (input_sample_rate as f64) / (output_sample_rate as f64);
        let left_index = source_position.floor() as usize;
        let right_index = left_index.saturating_add(1).min(samples.len() - 1);
        let fraction = (source_position - left_index as f64) as f32;
        let left = sanitize_sample(samples[left_index.min(samples.len() - 1)]);
        let right = sanitize_sample(samples[right_index]);
        output.push(left + (right - left) * fraction);
    }

    Ok(output)
}

pub fn encode_pcm16_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<u8>, RecordingError> {
    if sample_rate == 0 || channels == 0 {
        return Err(recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        ));
    }

    let data_len = samples.len().checked_mul(2).ok_or_else(|| {
        recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        )
    })?;
    let riff_chunk_size = 36usize.checked_add(data_len).ok_or_else(|| {
        recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        )
    })?;
    if riff_chunk_size > u32::MAX as usize || data_len > u32::MAX as usize {
        return Err(recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        ));
    }

    let block_align = channels
        .checked_mul(WAV_BITS_PER_SAMPLE / 8)
        .ok_or_else(|| {
            recording_error(
                RecordingErrorCode::WavEncodingFailed,
                "Recording WAV bytes could not be encoded.",
            )
        })?;
    let byte_rate = sample_rate.checked_mul(block_align as u32).ok_or_else(|| {
        recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        )
    })?;

    let mut wav = Vec::with_capacity(WAV_HEADER_LEN + data_len);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(riff_chunk_size as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&WAV_AUDIO_FORMAT_PCM.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&WAV_BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());

    for sample in samples {
        wav.extend_from_slice(&float_to_pcm16(*sample).to_le_bytes());
    }

    validate_pcm16_wav_header(&wav, sample_rate, channels, samples.len())?;
    Ok(wav)
}

pub fn float_to_pcm16(sample: f32) -> i16 {
    let clamped = sanitize_sample(sample);
    if clamped < 0.0 {
        (clamped * 32_768.0).round() as i16
    } else {
        (clamped * 32_767.0).round() as i16
    }
}

fn validate_pcm16_wav_header(
    wav: &[u8],
    sample_rate: u32,
    channels: u16,
    sample_count: usize,
) -> Result<(), RecordingError> {
    let expected_data_len = sample_count.checked_mul(2).ok_or_else(|| {
        recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        )
    })?;
    let expected_len = WAV_HEADER_LEN
        .checked_add(expected_data_len)
        .ok_or_else(|| {
            recording_error(
                RecordingErrorCode::WavEncodingFailed,
                "Recording WAV bytes could not be encoded.",
            )
        })?;
    let expected_riff_size = 36usize.checked_add(expected_data_len).ok_or_else(|| {
        recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        )
    })?;

    let is_valid = wav.len() == expected_len
        && wav.get(0..4) == Some(b"RIFF")
        && read_u32_le(wav, 4) == Some(expected_riff_size as u32)
        && wav.get(8..12) == Some(b"WAVE")
        && wav.get(12..16) == Some(b"fmt ")
        && read_u32_le(wav, 16) == Some(16)
        && read_u16_le(wav, 20) == Some(WAV_AUDIO_FORMAT_PCM)
        && read_u16_le(wav, 22) == Some(channels)
        && read_u32_le(wav, 24) == Some(sample_rate)
        && read_u32_le(wav, 28)
            == sample_rate.checked_mul((channels * (WAV_BITS_PER_SAMPLE / 8)) as u32)
        && read_u16_le(wav, 32) == Some(channels * (WAV_BITS_PER_SAMPLE / 8))
        && read_u16_le(wav, 34) == Some(WAV_BITS_PER_SAMPLE)
        && wav.get(36..40) == Some(b"data")
        && read_u32_le(wav, 40) == Some(expected_data_len as u32);

    if is_valid {
        Ok(())
    } else {
        Err(recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        ))
    }
}

pub fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
}

pub fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_agc, encode_pcm16_wav, encode_recording_wav, float_to_pcm16, read_u16_le,
        read_u32_le, resample_mono_linear, RecordingErrorCode, WAV_HEADER_LEN,
    };

    #[test]
    fn wav_header_matches_pcm16_mono_recording() {
        let wav =
            encode_pcm16_wav(&[-1.0, 0.0, 1.0], 16_000, 1).expect("wav encoding should succeed");

        assert_eq!(wav.len(), WAV_HEADER_LEN + 6);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(read_u32_le(&wav, 4), Some(42));
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(read_u32_le(&wav, 16), Some(16));
        assert_eq!(read_u16_le(&wav, 20), Some(1));
        assert_eq!(read_u16_le(&wav, 22), Some(1));
        assert_eq!(read_u32_le(&wav, 24), Some(16_000));
        assert_eq!(read_u32_le(&wav, 28), Some(32_000));
        assert_eq!(read_u16_le(&wav, 32), Some(2));
        assert_eq!(read_u16_le(&wav, 34), Some(16));
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(read_u32_le(&wav, 40), Some(6));
    }

    #[test]
    fn recording_wav_is_optimized_for_groq_upload() {
        let input: Vec<f32> = (0..48_000)
            .map(|index| if index % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let wav = encode_recording_wav(&input, 48_000).expect("wav encoding should succeed");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(read_u16_le(&wav, 20), Some(1));
        assert_eq!(read_u16_le(&wav, 22), Some(1));
        assert_eq!(read_u32_le(&wav, 24), Some(16_000));
        assert_eq!(read_u32_le(&wav, 28), Some(32_000));
        assert_eq!(read_u16_le(&wav, 32), Some(2));
        assert_eq!(read_u16_le(&wav, 34), Some(16));
        assert_eq!(read_u32_le(&wav, 40), Some(32_000));
        assert_eq!(wav.len(), WAV_HEADER_LEN + 32_000);
    }

    #[test]
    fn linear_resampler_preserves_duration_approximately() {
        let input = vec![0.25_f32; 48_000];
        let output =
            resample_mono_linear(&input, 48_000, 16_000).expect("resampling should succeed");

        assert_eq!(output.len(), 16_000);
        let input_duration = input.len() as f64 / 48_000.0;
        let output_duration = output.len() as f64 / 16_000.0;
        assert!((input_duration - output_duration).abs() < 0.001);
    }

    #[test]
    fn silence_encodes_as_valid_wav() {
        let wav = encode_recording_wav(&[0.0_f32; 480], 48_000)
            .expect("silence should encode successfully");
        let pcm: Vec<i16> = wav[WAV_HEADER_LEN..]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect();

        assert_eq!(read_u32_le(&wav, 24), Some(16_000));
        assert_eq!(pcm.len(), 160);
        assert!(pcm.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn wav_encoding_writes_clamped_pcm16_samples() {
        let wav = encode_pcm16_wav(&[-2.0, -0.5, 0.0, 0.5, 2.0], 48_000, 1)
            .expect("wav encoding should succeed");
        let pcm: Vec<i16> = wav[WAV_HEADER_LEN..]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect();

        assert_eq!(pcm, vec![-32768, -16384, 0, 16384, 32767]);
        assert_eq!(float_to_pcm16(f32::NAN), 0);
        assert_eq!(float_to_pcm16(f32::INFINITY), 0);
    }

    #[test]
    fn agc_boosts_quiet_signal() {
        let mut samples = vec![0.01_f32; 1000];
        apply_agc(&mut samples);

        let rms = (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64)
            .sqrt() as f32;
        assert!(rms > 0.01, "AGC should boost quiet signal, rms={rms}");
        assert!(rms <= 0.2, "AGC should not over-boost, rms={rms}");
    }

    #[test]
    fn agc_reduces_loud_signal() {
        let mut samples = vec![0.8_f32; 1000];
        apply_agc(&mut samples);

        let rms = (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64)
            .sqrt() as f32;
        assert!(rms < 0.8, "AGC should reduce loud signal, rms={rms}");
        assert!(rms > 0.05, "AGC should not silence signal, rms={rms}");
    }

    #[test]
    fn agc_skips_silence() {
        let mut samples = vec![0.0_f32; 100];
        apply_agc(&mut samples);

        assert!(samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn agc_clamps_output() {
        let mut samples = vec![-0.5_f32, 0.5];
        apply_agc(&mut samples);

        for s in &samples {
            assert!((-1.0..=1.0).contains(s), "sample {s} out of range");
        }
    }

    #[test]
    fn agc_handles_empty_slice() {
        let mut samples: Vec<f32> = vec![];
        apply_agc(&mut samples);
    }

    #[test]
    fn wav_encoding_rejects_invalid_format_parameters() {
        let sample_rate_error =
            encode_pcm16_wav(&[0.0], 0, 1).expect_err("zero sample rate should fail");
        let channels_error =
            encode_pcm16_wav(&[0.0], 48_000, 0).expect_err("zero channels should fail");

        assert_eq!(
            sample_rate_error.code,
            RecordingErrorCode::WavEncodingFailed
        );
        assert_eq!(channels_error.code, RecordingErrorCode::WavEncodingFailed);
    }
}
