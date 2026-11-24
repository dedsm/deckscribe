//! Everything that touches the video goes through the ffmpeg program: raw data
//! is read from its stdout, so nothing intermediate is written to disk.

use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

/// Analysis frames: grayscale, squashed to a fixed size, 5 per second.
pub const FW: usize = 480;
pub const FH: usize = 300;
pub const FPS: f64 = 5.0;
pub const AUDIO_RATE: usize = 16_000;

fn ffmpeg() -> Command {
    let mut c = Command::new("ffmpeg");
    c.args(["-v", "error", "-nostdin"]);
    c
}

fn spawn_err(e: std::io::Error) -> anyhow::Error {
    if e.kind() == ErrorKind::NotFound {
        anyhow::anyhow!("ffmpeg was not found on PATH; install it first")
    } else {
        e.into()
    }
}

/// Calls `each(index, frame)` for every analysis frame; returns the count.
pub fn gray_frames(video: &Path, mut each: impl FnMut(usize, &[u8])) -> Result<usize> {
    let mut child = ffmpeg()
        // Analysis frames need neither the deblocking filter nor the frames no
        // other frame depends on: skipping both decodes a 4K H.264 source
        // about 35% faster.
        .args(["-skip_loop_filter", "all", "-skip_frame", "noref", "-i"])
        .arg(video)
        .args(["-an", "-vf", &format!("fps={FPS},scale={FW}:{FH}:flags=area,format=gray")])
        .args(["-f", "rawvideo", "-"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(spawn_err)?;
    let mut out = BufReader::with_capacity(1 << 20, child.stdout.take().unwrap());
    let mut buf = vec![0u8; FW * FH];
    let mut n = 0;
    loop {
        match out.read_exact(&mut buf) {
            Ok(()) => {
                each(n, &buf);
                n += 1;
            }
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e).context("reading frames from ffmpeg"),
        }
    }
    ensure!(child.wait()?.success(), "ffmpeg failed to decode the video");
    Ok(n)
}

/// The soundtrack as 16 kHz mono samples in [-1, 1].
pub fn audio_16k(video: &Path) -> Result<Vec<f32>> {
    let mut child = ffmpeg()
        .arg("-i")
        .arg(video)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(spawn_err)?;
    let mut bytes = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut bytes)?;
    ensure!(child.wait()?.success(), "ffmpeg failed to decode the audio");
    Ok(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// Saves the full-resolution frame at `seconds` as a JPEG.
pub fn save_frame(video: &Path, seconds: f64, out: &Path) -> Result<()> {
    let status = ffmpeg()
        .args(["-y", "-ss", &format!("{seconds:.3}")])
        .arg("-i")
        .arg(video)
        .args(["-frames:v", "1", "-q:v", "2"])
        .arg(out)
        .status()
        .map_err(spawn_err)?;
    ensure!(status.success(), "ffmpeg failed to save the frame at {seconds:.1}s");
    Ok(())
}

/// An image re-encoded as JPEG at `width` pixels wide, in memory, with
/// `blank` regions (x, y, w, h as fractions of the frame) painted black.
pub fn jpeg_at_width(image: &Path, width: u32, blank: &[[f32; 4]]) -> Result<Vec<u8>> {
    let mut vf: Vec<String> = blank
        .iter()
        .map(|[x, y, w, h]| format!("drawbox=x=iw*{x}:y=ih*{y}:w=iw*{w}:h=ih*{h}:color=black:t=fill"))
        .collect();
    vf.push(format!("scale='min({width},iw)':-2"));
    let out = ffmpeg()
        .arg("-i")
        .arg(image)
        .args(["-vf", &vf.join(","), "-f", "image2pipe", "-c:v", "mjpeg", "-q:v", "3", "-"])
        .output()
        .map_err(spawn_err)?;
    ensure!(out.status.success(), "ffmpeg failed to scale {}", image.display());
    Ok(out.stdout)
}
