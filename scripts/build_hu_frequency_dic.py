"""Hermit Dave hu_50k.txt -> hunspell-style .dic (UTF-8, first column only).

Forrás (MIT): https://github.com/hermitdave/FrequencyWords
Letöltés: curl -sL 'https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/hu/hu_50k.txt' \\
  -o assets/hu_freq_50k_raw.txt
Majd: python scripts/build_hu_frequency_dic.py
"""
from __future__ import annotations

import unicodedata
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "assets" / "hu_freq_50k_raw.txt"
URL = "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/hu/hu_50k.txt"
OUT = ROOT / "assets" / "hu_frequency_hermitdave.dic"


def main() -> None:
    if not RAW.exists():
        print(f"downloading {URL} …")
        urllib.request.urlretrieve(URL, RAW)
    raw = RAW.read_text(encoding="utf-8", errors="replace")
    words: set[str] = set()
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        w = line.split()[0]
        if len(w) < 2 or len(w) > 64:
            continue
        ok = True
        for c in w:
            if c in "'-":
                ok = False
                break
            if not unicodedata.category(c).startswith("L"):
                ok = False
                break
        if ok:
            words.add(w.lower())
    lines = sorted(words)
    OUT.write_text(f"{len(lines)}\n" + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT.name}: {len(lines)} words")


if __name__ == "__main__":
    main()
