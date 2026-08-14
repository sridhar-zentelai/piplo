//! Whole file against 30 s pieces, on the same audio.
//!
//! The question this settles is whether splitting a long take into fixed-length
//! pieces is a latency win. Intuitively it looks like one — each piece is smaller,
//! and the pieces could overlap with recording. Measured on the sibling app it was
//! the opposite: nine 30 s pieces cost 2.4x the request time of the same audio
//! sent whole, because every piece re-pays a fixed per-request floor that dwarfs
//! the length-dependent part.
//!
//! This re-runs that on piplo's own client so the conclusion is not inherited.
//! Pieces are sent sequentially, which is what a chunking design would do if it
//! had to preserve word order in a single transcript.
//!
//! Run from `src-tauri`:
//!   cargo run --example chunk_ab -- <whole.wav> <chunk0.wav> <chunk1.wav> ...

use std::time::{Duration, Instant};

use reqwest::multipart::{Form, Part};
use reqwest::Client;

const BASE_URL: &str = "https://api.groq.com";
const WHISPER_MODEL: &str = "whisper-large-v3-turbo";
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: cargo run --example chunk_ab -- <whole.wav> <chunk0.wav> ...");
        std::process::exit(2);
    }

    let key = match load_key() {
        Some(key) => key,
        None => {
            eprintln!("no API key in credentials.json");
            std::process::exit(2);
        }
    };

    // The shared client, so neither side is charged for handshakes the app no
    // longer pays. Chunking would benefit most from reuse, so this is the
    // arrangement most favourable to it.
    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(600))
        .pool_max_idle_per_host(4)
        .tcp_keepalive(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("client");

    // Warm once, so the first request of either arm is not the one that pays.
    let _ = client
        .get(format!("{BASE_URL}/openai/v1/models"))
        .bearer_auth(&key)
        .timeout(Duration::from_secs(5))
        .send()
        .await;

    let whole_path = &args[0];
    let chunks = &args[1..];

    println!("=== WHOLE ===");
    let (whole_ms, whole_words, whole_secs) = send(&client, &key, whole_path).await;
    println!(
        "{:.1} s audio -> {} ms, {} words\n",
        whole_secs, whole_ms, whole_words
    );

    println!("=== {} PIECES ===", chunks.len());
    let mut total = 0u128;
    let mut words = 0usize;
    let mut secs = 0.0;
    for (i, path) in chunks.iter().enumerate() {
        let (ms, w, s) = send(&client, &key, path).await;
        total += ms;
        words += w;
        secs += s;
        println!("  piece {i}: {:.1} s -> {ms} ms, {w} words", s);
    }
    println!(
        "\n{:.1} s audio -> {} ms of request time, {} words",
        secs, total, words
    );

    println!("\n{}", "=".repeat(52));
    println!("whole   {whole_ms} ms");
    println!("pieces  {total} ms");
    if whole_ms > 0 {
        let factor = total as f64 / whole_ms as f64;
        println!(
            "pieces are {:.2}x the whole ({:+} ms)",
            factor,
            total as i128 - whole_ms as i128
        );
        println!(
            "per-piece average {:.0} ms — this is the fixed floor paid {} times",
            total as f64 / chunks.len() as f64,
            chunks.len()
        );
    }
}

async fn send(client: &Client, key: &str, path: &str) -> (u128, usize, f64) {
    let Ok(wav) = std::fs::read(path) else {
        eprintln!("  could not read {path}");
        return (0, 0, 0.0);
    };
    let seconds = wav_seconds(&wav);

    let part = Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .expect("mime");

    let form = Form::new()
        .part("file", part)
        .text("model", WHISPER_MODEL)
        .text("language", "en")
        .text("temperature", "0")
        .text("response_format", "verbose_json");

    let started = Instant::now();
    let response = client
        .post(format!("{BASE_URL}/openai/v1/audio/transcriptions"))
        .bearer_auth(key)
        .timeout(UPLOAD_TIMEOUT)
        .multipart(form)
        .send()
        .await;

    let (words, elapsed) = match response {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let elapsed = started.elapsed().as_millis();
            if !status.is_success() {
                eprintln!(
                    "  {} — {}",
                    status.as_u16(),
                    body.chars().take(160).collect::<String>()
                );
                (0, elapsed)
            } else {
                let words = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|json| {
                        Some(json.get("text")?.as_str()?.split_whitespace().count())
                    })
                    .unwrap_or(0);
                (words, elapsed)
            }
        }
        Err(err) => {
            eprintln!("  failed: {err}");
            (0, started.elapsed().as_millis())
        }
    };

    (elapsed, words, seconds)
}

fn load_key() -> Option<String> {
    let appdata = std::env::var("APPDATA").ok()?;
    let path = std::path::Path::new(&appdata)
        .join("com.codea.piplo")
        .join("credentials.json");
    let text = std::fs::read_to_string(path).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    json.get("apiKey")
        .or_else(|| json.get("api_key"))?
        .as_str()
        .map(str::to_string)
        .filter(|key| !key.trim().is_empty())
}

fn wav_seconds(wav: &[u8]) -> f64 {
    // Straight from the header rather than via hound, which would not read the
    // extra chunks some encoders leave in.
    if wav.len() < 44 {
        return 0.0;
    }
    let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let bits = u16::from_le_bytes([wav[34], wav[35]]) as u32;
    let channels = u16::from_le_bytes([wav[22], wav[23]]).max(1) as u32;
    let bytes_per_sample = (bits / 8).max(1) * channels;
    if rate == 0 {
        return 0.0;
    }
    (wav.len() as u32 - 44) as f64 / bytes_per_sample as f64 / rate as f64
}
