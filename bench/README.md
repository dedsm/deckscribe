# Benchmark

Scores deckscribe against human answers:

- **LPM** (Lecture Presentations Multimodal, CMU, ICCV 2023): hand-annotated
  slide changes (`segments.txt` per video). Scored as precision and recall of
  slide starts within 2 s and 5 s.
- **M3AV** (ACL 2024): human transcripts with word timestamps. Scored as word
  error rate and median word start error.

Neither dataset is included: both are CC BY-NC-SA 4.0 and their videos are on
YouTube under the uploaders' terms. To run the benchmark:

1. Download the LPM annotations (`lpmdata_102023.zip`, linked from
   github.com/dondongwon/LPMDataset) and M3AV's `dataset_v1.0_noaudio.tar.gz`
   (huggingface.co/datasets/CHHHH/M3AV_v1.0) into `$BENCH_DIR`, extracted to
   `lpm_archive/extracted/` and `m3av_archive/extracted/`.
2. Fetch the sample videos listed in `lpm_sample.tsv` and `m3av_sample.tsv`
   with `fetch.sh OUT.mp4 URL` (needs yt-dlp) into `$BENCH_DIR/videos/{lpm,m3av}/`.
3. `BENCH_DIR=... bench/run.sh NAME` then `BENCH_DIR=... python3 -I bench/score.py NAME`.

The sample: one LPM video per lecturer (10), one M3AV talk per source (5).
