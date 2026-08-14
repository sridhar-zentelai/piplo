//! Does Groq's Whisper actually obey the `prompt` field?
//!
//! The vocabulary hint is the only mechanism that can make Whisper produce a term
//! Piplo has never seen a mishearing for. Ten dictations in a row came back wrong
//! with the hint attached, which leaves two very different explanations:
//!
//! 1. The hint is delivered and Whisper simply loses to the acoustics.
//! 2. The hint is delivered and Groq ignores the field.
//!
//! Guessing between those is worthless — one is a fact of life, the other is a
//! reason to stop building on the hint at all. So this replays **the same audio**
//! against several prompt shapes and prints what comes back.
//!
//! ```
//! set PIPLO_DUMP_WAV=%TEMP%\piplo-sample.wav   (then dictate once)
//! cargo run --example prompt_ab -- "%TEMP%\piplo-sample.wav" ZentelAI [trials]
//! ```

use std::time::Duration;

use reqwest::multipart::{Form, Part};
use reqwest::Client;

const BASE_URL: &str = "https://api.groq.com";
const WHISPER_MODEL: &str = "whisper-large-v3-turbo";

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);

    let Some(wav_path) = args.next() else {
        eprintln!("usage: cargo run --example prompt_ab -- \"<path to wav>\" <term> [trials]");
        std::process::exit(2);
    };

    let term = args.next().unwrap_or_else(|| "ZentelAI".to_string());
    let trials: usize = args
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3)
        .max(1);

    let Some(key) = api_key() else {
        eprintln!("no key: set GROQ_API_KEY, put one in .env, or save one in Settings");
        std::process::exit(2);
    };

    let wav = match std::fs::read(&wav_path) {
        Ok(wav) => wav,
        Err(err) => {
            eprintln!("could not read {wav_path}: {err}");
            std::process::exit(2);
        }
    };

    println!(
        "file: {wav_path} ({:.0} kB), term: {term}, {trials} trials each\n",
        wav.len() as f64 / 1024.0
    );

    // Four shapes, because "the prompt does nothing" and "the prompt does nothing
    // *in this form*" are different findings and only a comparison separates them.
    let shapes: Vec<(&str, Option<String>)> = vec![
        ("bare term (the old behaviour)", Some(term.clone())),
        // What `vocabulary::CARRIER` builds right now. Short, and short sentences
        // have failed before — which is the thing to find out rather than assume.
        ("SHIPPING: The team uses …", Some(format!("The team uses {term}."))),
        // Known good from the previous run, as the yardstick.
        (
            "known good: longer sentence",
            Some(format!("The team at {term} shipped it yesterday.")),
        ),
        // Candidate carriers, all with the term last and varying only in how much
        // natural text precedes it.
        ("candidate: we are working on", Some(format!("We are working on {term}."))),
        (
            "candidate: today we talked about",
            Some(format!("Today we talked about {term}.")),
        ),
        (
            "candidate: the team has been working on",
            Some(format!("The team has been working on {term}.")),
        ),
    ];

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("client");

    for (label, prompt) in shapes {
        println!("── {label}");

        if let Some(prompt) = &prompt {
            println!("   prompt: {prompt:?}");
        }

        for trial in 1..=trials {
            let text = transcribe(&client, &key, &wav, prompt.clone()).await;
            let hit = text.to_lowercase().contains(&term.to_lowercase());
            println!("   {trial}. {} {text}", if hit { "HIT " } else { "miss" });

            // Groq's on-demand tier allows 20 requests a minute, and a run that
            // spends half its rows on 429s measures nothing.
            tokio::time::sleep(Duration::from_millis(3200)).await;
        }

        println!();
    }
}

/// The same three places the app looks, in the same order: the environment, the
/// repo `.env`, then the key saved from Settings. Nothing here prints it.
fn api_key() -> Option<String> {
    let from_env = |()| match std::env::var("GROQ_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(key),
        _ => None,
    };

    if let Some(key) = from_env(()) {
        return Some(key);
    }

    let _ = dotenvy::from_path("../.env").or_else(|_| dotenvy::dotenv().map(|_| ()));

    if let Some(key) = from_env(()) {
        return Some(key);
    }

    // `%APPDATA%\com.codea.piplo\credentials.json`, written by Settings. Read
    // straight through to the request — it is never logged or echoed.
    let dir = std::env::var("APPDATA").ok()?;
    let text = std::fs::read_to_string(
        std::path::Path::new(&dir)
            .join("com.codea.piplo")
            .join("credentials.json"),
    )
    .ok()?;

    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("apiKey")?
        .as_str()
        .map(str::to_owned)
        .filter(|key| !key.trim().is_empty())
}

async fn transcribe(client: &Client, key: &str, wav: &[u8], prompt: Option<String>) -> String {
    let part = Part::bytes(wav.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .expect("mime");

    let mut form = Form::new()
        .part("file", part)
        .text("model", WHISPER_MODEL)
        .text("language", "en")
        .text("temperature", "0")
        .text("response_format", "verbose_json");

    if let Some(prompt) = prompt {
        form = form.text("prompt", prompt);
    }

    let response = client
        .post(format!("{BASE_URL}/openai/v1/audio/transcriptions"))
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await;

    match response {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if !status.is_success() {
                return format!("<{}: {}>", status.as_u16(), body.trim());
            }

            serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|json| json.get("text")?.as_str().map(str::to_owned))
                .unwrap_or_else(|| "<unparseable>".into())
                .trim()
                .to_string()
        }
        Err(err) => format!("<network: {err}>"),
    }
}
