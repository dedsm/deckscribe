#!/usr/bin/env bash
# usage: fetch.sh OUT.mp4 URL  -> <=480p avc1 video + m4a audio, merged
set -u
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
YTDLP=/home/david/Develop/personal/POCs/tmp/slidegeist-run/.venv/bin/yt-dlp
out=$1; url=$2
[ -s "$out" ] && { echo "have $out"; exit 0; }
$YTDLP --no-warnings --no-playlist -q -f 'bv*[height<=480][vcodec^=avc1]+ba[ext=m4a]/b[height<=480][ext=mp4]/bv*[height<=480]+ba' \
  --merge-output-format mp4 -o "$out" "$url" && echo "ok $out" || echo "FAILED $out"
