#!/usr/bin/env bash
# usage: sheet.sh RUN_DIR OUT.jpg [COLS]  -> contact sheet of a run's slide images, in order
set -eu
dir=$1; out=$2; cols=${3:-5}
n=$(ls "$dir"/slides/slide_*.jpg | wc -l); rows=$(( (n + cols - 1) / cols ))
ffmpeg -v error -y -pattern_type glob -i "$dir/slides/slide_*.jpg" -vf "scale=320:180:force_original_aspect_ratio=decrease,pad=324:184:(ow-iw)/2:(oh-ih)/2:color=gray,tile=${cols}x${rows}" -frames:v 1 "$out"
