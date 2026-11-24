//! Visual segmentation on synthetic frames: slides, overlays, live stretches.

use deckscribe_core::Settings;
use deckscribe_core::frames::{FH, FPS, FW, FrameFeatures, GH, GW};
use deckscribe_core::segmentation::{Found, Layout, segment};

/// A small deterministic random generator.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

/// A light slide with dark text-like bars placed by `id`.
fn slide(id: u64) -> Vec<u8> {
    let mut f = vec![235u8; FW * FH];
    let mut r = Lcg(id * 7919 + 1);
    for _ in 0..14 {
        let (x, y) = (20 + r.next() as usize % 380, 20 + r.next() as usize % 220);
        let (w, h) = (40 + r.next() as usize % 80, 8 + r.next() as usize % 14);
        fill(&mut f, x, y, w, h, 30);
    }
    f
}

fn fill(f: &mut [u8], x: usize, y: usize, w: usize, h: usize, v: u8) {
    for yy in y..(y + h).min(FH) {
        for xx in x..(x + w).min(FW) {
            f[yy * FW + xx] = v;
        }
    }
}

/// Fresh noise in a box, as a webcam picture changes from frame to frame.
fn noise(f: &mut [u8], seed: u64, x: usize, y: usize, w: usize, h: usize) {
    let mut r = Lcg(seed);
    for yy in y..y + h {
        for xx in x..x + w {
            f[yy * FW + xx] = (r.next() % 256) as u8;
        }
    }
}

fn seconds(s: f64) -> usize {
    (s * FPS) as usize
}

fn features(frames: impl IntoIterator<Item = Vec<u8>>) -> FrameFeatures {
    let mut ff = FrameFeatures::new();
    for f in frames {
        ff.push(&f);
    }
    ff
}

fn slides(ff: &FrameFeatures, s: &Settings) -> Vec<(Found, f64, f64)> {
    segment(ff, s, ff.seconds())
        .parts
        .iter()
        .map(|p| (p.kind, p.span.start, p.span.end))
        .collect()
}

#[test]
fn two_slides_and_their_boundary() {
    let frames = (0..seconds(20.0)).map(|i| slide(if i < seconds(10.0) { 1 } else { 2 }));
    let ff = features(frames);
    let parts = slides(&ff, &Settings::default());
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        (parts[1].1 - 10.0).abs() < 0.5,
        "second slide starts at {}",
        parts[1].1
    );
    assert_eq!(parts[1].2, 20.0);
}

#[test]
fn a_still_shorter_than_the_minimum_is_not_a_slide() {
    let frames = (0..seconds(24.0)).map(|i| {
        let t = i as f64 / FPS;
        slide(if t < 10.0 {
            1
        } else if t < 14.0 {
            2
        } else {
            3
        })
    });
    let parts = slides(&features(frames), &Settings::default());
    assert_eq!(
        parts.len(),
        2,
        "the 4 s slide folds into the next: {parts:?}"
    );
}

#[test]
fn a_webcam_is_masked_and_does_not_cut() {
    let frames = (0..seconds(40.0)).map(|i| {
        let mut f = slide(if i < seconds(20.0) { 1 } else { 2 });
        noise(&mut f, i as u64, 380, 200, 90, 90);
        f
    });
    let ff = features(frames);
    let seg = segment(&ff, &Settings::default(), ff.seconds());
    let boxes = &seg.stretches.list[0].overlays;
    assert_eq!(boxes.len(), 1, "{boxes:?}");
    let b = boxes[0];
    // 380..470 x 200..290 px is blocks 38..47 x 20..29, plus one block of padding.
    assert!(
        b.x0 <= 38 && b.x1 >= 47 && b.y0 <= 20 && b.y1 >= 29,
        "{b:?}"
    );
    assert!(
        b.x1 - b.x0 < GW / 3 && b.y1 - b.y0 < GH / 2,
        "the box stays small: {b:?}"
    );
    assert_eq!(seg.parts.len(), 2, "{:?}", seg.parts);
}

#[test]
fn without_masking_a_webcam_breaks_every_slide() {
    let frames = (0..seconds(40.0)).map(|i| {
        let mut f = slide(1);
        noise(&mut f, i as u64, 300, 120, 170, 170);
        f
    });
    let ff = features(frames);
    let s = Settings {
        find_overlays: false,
        ..Settings::default()
    };
    let parts = slides(&ff, &s);
    assert!(parts.iter().all(|p| p.0 == Found::Slide));
    assert_eq!(
        parts.len(),
        1,
        "no hold is long enough, so one fallback part: {parts:?}"
    );
}

#[test]
fn changing_captions_are_masked() {
    let frames = (0..seconds(40.0)).map(|i| {
        let mut f = slide(if i < seconds(20.0) { 1 } else { 2 });
        // A dark caption box whose words change every 2.5 s.
        fill(&mut f, 120, 250, 240, 40, 40);
        let mut words = Lcg(1000 + (i / seconds(2.5)) as u64);
        for _ in 0..8 {
            let (x, y) = (
                124 + words.next() as usize % 200,
                254 + words.next() as usize % 20,
            );
            fill(&mut f, x, y, 14 + words.next() as usize % 20, 12, 250);
        }
        f
    });
    let ff = features(frames);
    let seg = segment(&ff, &Settings::default(), ff.seconds());
    assert!(!seg.stretches.list[0].overlays.is_empty(), "captions found");
    assert_eq!(seg.parts.len(), 2, "{:?}", seg.parts);
}

#[test]
fn a_camera_stretch_becomes_a_live_part() {
    let frames = (0..seconds(60.0)).map(|i| {
        let t = i as f64 / FPS;
        if (20.0..40.0).contains(&t) {
            let mut f = vec![0u8; FW * FH];
            noise(&mut f, i as u64, 0, 0, FW, FH);
            f
        } else {
            slide(if t < 20.0 { 1 } else { 2 })
        }
    });
    let ff = features(frames);
    let seg = segment(&ff, &Settings::default(), ff.seconds());
    let kinds: Vec<Found> = seg.parts.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [Found::Slide, Found::Live, Found::Slide],
        "{:?}",
        seg.parts
    );
    assert!(seg.stretches.list.iter().any(|s| s.layout == Layout::Live));
    // Without live parts the camera stretch folds into the next slide.
    let s = Settings {
        live_parts: false,
        ..Settings::default()
    };
    assert_eq!(slides(&ff, &s).len(), 2);
}

#[test]
fn the_same_slide_after_an_interruption_is_merged() {
    let frames = (0..seconds(21.0)).map(|i| {
        let t = i as f64 / FPS;
        if (10.0..10.4).contains(&t) {
            vec![0u8; FW * FH]
        } else {
            slide(1)
        }
    });
    let parts = slides(&features(frames), &Settings::default());
    assert_eq!(parts.len(), 1, "{parts:?}");
}

#[test]
fn parts_tile_the_recording_and_images_lie_inside() {
    let frames = (0..seconds(30.0)).map(|i| slide(1 + (i / seconds(10.0)) as u64));
    let ff = features(frames);
    let seg = segment(&ff, &Settings::default(), 31.0);
    assert_eq!(seg.parts.first().unwrap().span.start, 0.0);
    assert_eq!(seg.parts.last().unwrap().span.end, 31.0);
    for w in seg.parts.windows(2) {
        assert_eq!(w[0].span.end, w[1].span.start);
    }
    for p in &seg.parts {
        assert!(p.span.contains(p.image_at), "{p:?}");
        assert!(
            p.image_at <= p.span.end - Settings::default().image_lead + 0.21,
            "image taken before the hold ends: {p:?}"
        );
    }
}
