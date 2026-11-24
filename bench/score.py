"""Score slidewords runs against LPM's hand-annotated slide changes and M3AV's transcripts.

usage: BENCH_DIR=... python3 -I bench/score.py RUN_NAME
"""
import csv, json, os, re, statistics, sys
from difflib import SequenceMatcher
from pathlib import Path

base = Path(os.environ.get("BENCH_DIR", Path(__file__).resolve().parent))
run = base / "runs" / sys.argv[1]

def starts(d):
    """Part start times from a deck (deck.json) or an older run (slides.json)."""
    if (d / "deck.json").exists():
        return [p["start"] for p in json.loads((d / "deck.json").read_text())["parts"]]
    return [p["start"] for p in json.loads((d / "slides.json").read_text())]

def load_words(d):
    raw = json.loads((d / "words.json").read_text())
    return [w if isinstance(w, dict) else {"start": w[0], "end": w[1], "text": w[2]} for w in raw]

def match(pred, truth, tol):
    """One-to-one greedy matching of times within tol; returns hits."""
    used, hits = set(), 0
    for p in pred:
        best = min(((abs(p - t), i) for i, t in enumerate(truth) if i not in used and abs(p - t) <= tol), default=None)
        if best:
            used.add(best[1]); hits += 1
    return hits

print("== LPM slide changes (hand-annotated)")
print(f"{'video':28s} {'truth':>5s} {'found':>5s}  {'P@2s':>5s} {'R@2s':>5s}  {'P@5s':>5s} {'R@5s':>5s}  overlays")
tot = {2: [0, 0, 0], 5: [0, 0, 0]}
for line in (base / "lpm_sample.tsv").read_text().splitlines():
    sp, vid, url, seg, secs, n = line.split("\t")
    d = run / "lpm" / f"{sp}_{vid}"
    if not ((d / "deck.json").exists() or (d / "slides.json").exists()):
        print(f"{sp+'_'+vid:28s} not run"); continue
    truth = [float(x) for x in Path(base / seg).read_text().split()]
    pred = starts(d)[1:]
    masks = [l.split(": ", 1)[1] for l in (d / "run.log").read_text().splitlines() if l.startswith("masking")]
    row = []
    for tol in (2, 5):
        h = match(pred, truth, tol)
        tot[tol][0] += h; tot[tol][1] += len(pred); tot[tol][2] += len(truth)
        row += [h / len(pred) if pred else 0, h / len(truth) if truth else 0]
    print(f"{sp+'_'+vid:28s} {len(truth):5d} {len(pred):5d}  {row[0]:5.2f} {row[1]:5.2f}  {row[2]:5.2f} {row[3]:5.2f}  {'; '.join(masks) or '-'}")
for tol in (2, 5):
    h, p, t = tot[tol]
    if p and t:
        P, R = h / p, h / t
        print(f"TOTAL @{tol}s: precision {P:.2f} recall {R:.2f} F1 {2*P*R/(P+R):.2f} ({h} hits, {p} found, {t} annotated)")

def norm(s):
    return re.findall(r"[a-z0-9']+", s.lower().replace("’", "'"))

def wer(ref, hyp):
    prev = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        cur = [i] + [0] * len(hyp)
        for j, h in enumerate(hyp, 1):
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (r != h))
        prev = cur
    return prev[-1] / max(1, len(ref))

print("\n== M3AV transcripts (human-checked text, word timestamps)")
print(f"{'video':12s} {'ref words':>9s} {'WER':>6s}  {'start err median':>16s} {'p90':>6s}")
for line in (base / "m3av_sample.tsv").read_text().splitlines():
    src, vid, url, secs = line.split("\t")
    d = run / "m3av" / vid
    if not (d / "words.json").exists():
        print(f"{vid:12s} not run"); continue
    segs = json.loads((base / f"m3av_archive/extracted/dataset_v1.0_noaudio/{vid}/speech/final+timestamps.json").read_text())
    spans = [(int(s["timestr"][:7]) / 1000, int(s["timestr"][8:]) / 1000) for s in segs]
    ref = [w for s in segs for w in norm(s["final_written"])]
    words = load_words(d)
    # Only words inside the stretches the reference covers.
    inside = [w for w in words if any(a - 0.5 <= (w["start"] + w["end"]) / 2 <= b + 0.5 for a, b in spans)]
    hyp = [t for w in inside for t in norm(w["text"])]
    # Timing: match our words to the reference's spoken words by text.
    rs = [(t, w["start"]) for s in segs for w in s["words_spoken"] for t in norm(w["word"])[:1]]
    hs = [(t, w["start"]) for w in inside for t in norm(w["text"])[:1]]
    sm = SequenceMatcher(None, [x[0] for x in rs], [x[0] for x in hs], autojunk=False)
    errs = [abs(rs[b.a + k][1] - hs[b.b + k][1]) for b in sm.get_matching_blocks() for k in range(b.size)]
    med = statistics.median(errs) if errs else float("nan")
    p90 = sorted(errs)[int(0.9 * (len(errs) - 1))] if errs else float("nan")
    print(f"{vid:12s} {len(ref):9d} {wer(ref, hyp):6.3f}  {med:15.2f}s {p90:5.2f}s")
