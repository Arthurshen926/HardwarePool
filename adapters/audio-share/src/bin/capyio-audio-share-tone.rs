use std::{
    env,
    f32::consts::TAU,
    net::SocketAddr,
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use capyio_audio::AudioFormat;
use capyio_audio_share_adapter::{
    AudioSharePrivateFormat, AudioShareTransport, AudioShareTransportConfig,
    AudioShareTransportError,
};

const SAMPLE_RATE_HZ: u32 = 48_000;
const SAMPLES_PER_BLOCK: usize = 480;
const TONE_HZ: f32 = 440.0;
const TONE_AMPLITUDE: f32 = 0.18;
const RECEIVER_DEADLINE: Duration = Duration::from_secs(60);
const PLAY_DURATION: Duration = Duration::from_secs(10);
const BLOCK_DURATION: Duration = Duration::from_millis(10);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-audio-share-tone".to_owned());
    let Some(bind) = args.next() else {
        return Err(format!("usage: {executable} <explicit-ipv4:port>"));
    };
    if args.next().is_some() {
        return Err(format!("usage: {executable} <explicit-ipv4:port>"));
    }
    let bind_address = bind
        .parse::<SocketAddr>()
        .map_err(|_| "bind address must be an explicit IPv4 socket address".to_owned())?;
    let format = AudioSharePrivateFormat::from_audio_format(&AudioFormat::speaker_baseline())
        .map_err(|error| error.to_string())?;
    let transport =
        AudioShareTransport::bind(AudioShareTransportConfig::local_lab(bind_address), format)
            .map_err(|error| error.to_string())?;
    println!(
        "listening={} format=s16le/48000/stereo; start Audio Share on Android",
        transport.local_address()
    );

    let receiver_deadline = Instant::now() + RECEIVER_DEADLINE;
    while transport.connected_receivers() == 0 {
        if Instant::now() >= receiver_deadline {
            return Err("no Android receiver completed TCP/UDP association within 60s".to_owned());
        }
        thread::sleep(Duration::from_millis(50));
    }
    println!("receiver_connected=true; sending 440Hz tone for 10s");

    let sender = transport.sender();
    let mut first_sample_index = 0_u64;
    let end = Instant::now() + PLAY_DURATION;
    let mut next_block = Instant::now();
    while Instant::now() < end {
        let pcm = tone_block(first_sample_index);
        match sender.try_send_pcm(&pcm) {
            Ok(()) | Err(AudioShareTransportError::QueueFull) => {}
            Err(error) => return Err(error.to_string()),
        }
        first_sample_index += SAMPLES_PER_BLOCK as u64;
        next_block += BLOCK_DURATION;
        if let Some(remaining) = next_block.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    }
    let stats = transport.stats();
    println!(
        "tone_complete=true blocks_enqueued={} queue_full={} blocks_without_receiver={} datagrams_sent={} datagram_send_errors={} pcm_bytes_sent={}",
        stats.blocks_enqueued,
        stats.queue_full,
        stats.blocks_without_receiver,
        stats.datagrams_sent,
        stats.datagram_send_errors,
        stats.pcm_bytes_sent,
    );
    transport.shutdown();
    Ok(())
}

fn tone_block(first_sample_index: u64) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(SAMPLES_PER_BLOCK * 2 * size_of::<i16>());
    for offset in 0..SAMPLES_PER_BLOCK {
        let sample_index = first_sample_index + offset as u64;
        let phase = TAU * TONE_HZ * sample_index as f32 / SAMPLE_RATE_HZ as f32;
        let sample = (phase.sin() * TONE_AMPLITUDE * f32::from(i16::MAX)) as i16;
        pcm.extend_from_slice(&sample.to_le_bytes());
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    pcm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_block_is_one_aligned_non_silent_baseline_frame() {
        let pcm = tone_block(0);
        assert_eq!(pcm.len(), SAMPLES_PER_BLOCK * 4);
        assert!(pcm.chunks_exact(2).any(|sample| sample != [0, 0]));
        assert!(pcm.chunks_exact(4).all(|frame| frame[..2] == frame[2..]));
    }
}
