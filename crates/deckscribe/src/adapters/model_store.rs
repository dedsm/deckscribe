//! The speech model is not shipped: it is downloaded once, from a pinned
//! revision, into the platform's cache folder and checked against a known hash.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use deckscribe_core::ports::ModelStore;
use sha2::{Digest, Sha256};

const NAME: &str = "parakeet-tdt-0.6b-v2-Q8_0.gguf";
const URL: &str = "https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v2-gguf/resolve/58239cf6d0ecb0af921e7936c1e5e0a8dba91dbb/parakeet-tdt-0.6b-v2-Q8_0.gguf";
const SHA256: &str = "f0d0e99cebb6d3b83f1f7069b82b5d3c2e39a54545b0da039cb4bafd9c4e5caa";

/// A model file given by the user, or the default one from the cache.
pub struct HuggingFaceStore {
    pub given: Option<PathBuf>,
}

impl ModelStore for HuggingFaceStore {
    fn speech_model(&self) -> Result<PathBuf> {
        if let Some(p) = &self.given {
            if !p.is_file() {
                bail!("model file {} does not exist", p.display());
            }
            return Ok(p.clone());
        }
        let dir = dirs::cache_dir().context("no cache folder on this system; pass --model")?.join("deckscribe").join("models");
        let path = dir.join(NAME);
        if path.is_file() {
            return Ok(path);
        }
        fs::create_dir_all(&dir)?;
        download(&path)?;
        Ok(path)
    }
}

fn download(path: &Path) -> Result<()> {
    eprintln!("Downloading Parakeet TDT 0.6B v2 by NVIDIA (CC BY 4.0), about 700 MB, to {}", path.display());
    let partial = path.with_extension("gguf.partial");
    let mut response = ureq::get(URL).call().context("downloading the speech model")?;
    let mut body = response.body_mut().as_reader();
    let mut file = File::create(&partial)?;
    let mut hasher = Sha256::new();
    let (mut buf, mut total, mut next_report) = (vec![0u8; 1 << 20], 0u64, 0u64);
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        total += n as u64;
        if total >= next_report {
            eprintln!("  {} MB", total >> 20);
            next_report += 100 << 20;
        }
    }
    file.sync_all()?;
    let got = format!("{:x}", hasher.finalize());
    if got != SHA256 {
        let _ = fs::remove_file(&partial);
        bail!("the downloaded model's hash is {got}, expected {SHA256}");
    }
    fs::rename(&partial, path)?;
    Ok(())
}
