//! The ffmpeg program as the source of frames, audio and still images. Raw
//! data is read from its stdout, so nothing intermediate is written to disk.

use std::fmt::Write as _;
use std::io::{BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use deckscribe_core::frames::{FH, FPS, FW};
use deckscribe_core::ports::{AudioSource, FrameSource, StillSource};

pub struct Ffmpeg {
    recording: PathBuf,
}

impl Ffmpeg {
    pub fn new(recording: impl Into<PathBuf>) -> Self {
        Ffmpeg {
            recording: recording.into(),
        }
    }
}

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

impl FrameSource for Ffmpeg {
    fn frames(&self, each: &mut dyn FnMut(&[u8])) -> Result<usize> {
        let mut child = ffmpeg()
            // Analysis frames need neither the deblocking filter nor the frames
            // no other frame depends on; skipping both decodes a 4K H.264
            // source about 35% faster.
            .args(["-skip_loop_filter", "all", "-skip_frame", "noref", "-i"])
            .arg(&self.recording)
            .args([
                "-an",
                "-vf",
                &format!("fps={FPS},scale={FW}:{FH}:flags=area,format=gray"),
            ])
            .args(["-f", "rawvideo", "-"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(spawn_err)?;
        let mut out = BufReader::with_capacity(1 << 20, child.stdout.take().expect("piped stdout"));
        let mut buf = vec![0u8; FW * FH];
        let mut n = 0;
        loop {
            match out.read_exact(&mut buf) {
                Ok(()) => {
                    each(&buf);
                    n += 1;
                }
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e).context("reading frames from ffmpeg"),
            }
        }
        ensure!(
            child.wait()?.success(),
            "ffmpeg could not decode the picture of {}",
            self.recording.display()
        );
        Ok(n)
    }
}

impl AudioSource for Ffmpeg {
    fn audio(&self) -> Result<Vec<f32>> {
        let mut child = ffmpeg()
            .arg("-i")
            .arg(&self.recording)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(spawn_err)?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .read_to_end(&mut bytes)?;
        ensure!(
            child.wait()?.success(),
            "ffmpeg could not decode the sound of {}",
            self.recording.display()
        );
        Ok(bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect())
    }
}

impl StillSource for Ffmpeg {
    fn save_still(&self, at: f64, out: &Path) -> Result<()> {
        let status = ffmpeg()
            .args(["-y", "-ss", &format!("{at:.3}")])
            .arg("-i")
            .arg(&self.recording)
            .args(["-frames:v", "1", "-q:v", "2"])
            .arg(out)
            .status()
            .map_err(spawn_err)?;
        ensure!(
            status.success(),
            "ffmpeg could not save the picture at {at:.1}s"
        );
        Ok(())
    }

    fn prepared(&self, image: &Path, width: u32, blank: &[[f32; 4]]) -> Result<Vec<u8>> {
        let mut vf: Vec<String> = blank
            .iter()
            .map(|[x, y, w, h]| {
                format!("drawbox=x=iw*{x}:y=ih*{y}:w=iw*{w}:h=ih*{h}:color=black:t=fill")
            })
            .collect();
        vf.push(format!("scale='min({width},iw)':-2"));
        let out = ffmpeg()
            .arg("-i")
            .arg(image)
            .args([
                "-vf",
                &vf.join(","),
                "-f",
                "image2pipe",
                "-c:v",
                "mjpeg",
                "-q:v",
                "3",
                "-",
            ])
            .output()
            .map_err(spawn_err)?;
        ensure!(
            out.status.success(),
            "ffmpeg could not prepare {}",
            image.display()
        );
        Ok(out.stdout)
    }

    fn contact_sheet(
        &self,
        images: &[(PathBuf, String)],
        columns: usize,
        out: &Path,
    ) -> Result<()> {
        ensure!(!images.is_empty(), "no images for a contact sheet");
        let n = images.len();
        let cols = columns.clamp(1, n);
        let rows = n.div_ceil(cols);
        let run = |labels: bool| -> Result<bool> {
            let mut cmd = ffmpeg();
            cmd.arg("-y");
            for (p, _) in images {
                cmd.arg("-i").arg(p);
            }
            let mut graph = String::new();
            for (k, (_, label)) in images.iter().enumerate() {
                write!(
                    graph,
                    "[{k}:v]scale=400:224:force_original_aspect_ratio=decrease:force_divisible_by=2,\
                     setsar=1,pad=400:224:(ow-iw)/2:(oh-ih)/2:color=0x808080"
                )?;
                if labels {
                    let text: String = label
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || " #".contains(*c))
                        .collect();
                    write!(
                        graph,
                        ",drawtext=text='{text}':fontsize=24:fontcolor=white:\
                         box=1:boxcolor=black@0.75:boxborderw=6:x=10:y=10"
                    )?;
                }
                write!(graph, "[v{k}];")?;
            }
            for k in 0..n {
                write!(graph, "[v{k}]")?;
            }
            write!(
                graph,
                "concat=n={n}:v=1:a=0,tile={cols}x{rows}:padding=6:color=0x808080[out]"
            )?;
            cmd.args([
                "-filter_complex",
                &graph,
                "-map",
                "[out]",
                "-frames:v",
                "1",
                "-q:v",
                "3",
            ])
            .arg(out)
            .stderr(if labels {
                Stdio::null()
            } else {
                Stdio::inherit()
            });
            Ok(cmd.status().map_err(spawn_err)?.success())
        };
        // Numbered chips need a font; without one, the sheet still comes out.
        if !run(true)? {
            ensure!(run(false)?, "ffmpeg could not make the contact sheet");
        }
        Ok(())
    }
}
