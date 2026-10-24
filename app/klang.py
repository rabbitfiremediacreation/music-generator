"""Klang-Nachbearbeitung der fertigen Songs.

Höhen ausgleichen: Der Klang-Decoder (VAE) von ACE-Step 1.5 verliert über 16 kHz rund 8 dB und zwischen
8 und 16 kHz knapp 3 dB. Gemessen, indem ein fertiges Musikstück nur durch den VAE kodiert und wieder
dekodiert wurde. Keine Synthese-Einstellung ändert daran viel (XL bringt etwa 2 dB). Die Songs klingen
dadurch dumpf, „wie eine alte Kassette“. Ausgleichen lässt sich das nur danach: ein sanfter High-Shelf
(+6 dB ab 12 kHz, wie ffmpeg „treble=g=6:f=12000:t=s:w=0.7“), danach die Spitze auf 0,95 begrenzen.
"""
import io
import math
import wave

import numpy as np

SHELF_GAIN_DB = 6.0
SHELF_FREQ = 12000.0
SHELF_SLOPE = 0.7
PEAK = 0.95
_IR_LEN = 4096       # Impulsantwort des Filters; klingt nach wenigen Millisekunden aus
_BLOCK = 1 << 16


def _shelf(fs: int) -> tuple[list[float], list[float]]:
    """Biquad-High-Shelf nach dem Audio-EQ-Cookbook (R. Bristow-Johnson), Steilheit S wie ffmpeg „treble“."""
    a = 10 ** (SHELF_GAIN_DB / 40)
    w0 = 2 * math.pi * SHELF_FREQ / fs
    cw, sw = math.cos(w0), math.sin(w0)
    alpha = sw / 2 * math.sqrt((a + 1 / a) * (1 / SHELF_SLOPE - 1) + 2)
    sa = 2 * math.sqrt(a) * alpha
    b = [a * ((a + 1) + (a - 1) * cw + sa), -2 * a * ((a - 1) + (a + 1) * cw), a * ((a + 1) + (a - 1) * cw - sa)]
    den = [(a + 1) - (a - 1) * cw + sa, 2 * ((a - 1) - (a + 1) * cw), (a + 1) - (a - 1) * cw - sa]
    return [x / den[0] for x in b], [x / den[0] for x in den]


def _impulse(fs: int) -> np.ndarray:
    b, a = _shelf(fs)
    h = np.zeros(_IR_LEN)
    x1 = x2 = y1 = y2 = 0.0
    for n in range(_IR_LEN):
        x0 = 1.0 if n == 0 else 0.0
        y0 = b[0] * x0 + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2
        h[n] = y0
        x2, x1, y2, y1 = x1, x0, y1, y0
    return h


def _convolve(x: np.ndarray, h: np.ndarray) -> np.ndarray:
    """Overlap-Add mit FFT, blockweise, damit auch ein 10-Minuten-Song wenig Speicher braucht."""
    n_fft = 1 << int(math.ceil(math.log2(_BLOCK + len(h) - 1)))
    hf = np.fft.rfft(h, n_fft)
    out = np.zeros(len(x) + len(h) - 1)
    for start in range(0, len(x), _BLOCK):
        seg = x[start:start + _BLOCK]
        y = np.fft.irfft(np.fft.rfft(seg, n_fft) * hf, n_fft)[:len(seg) + len(h) - 1]
        out[start:start + len(y)] += y
    return out[:len(x)]


def hoehen_ausgleichen(wav_bytes: bytes) -> bytes:
    """16-Bit-WAV rein, 16-Bit-WAV raus (gleiche Rate und Kanäle). Andere Formate bleiben unverändert."""
    with wave.open(io.BytesIO(wav_bytes)) as w:
        ch, width, fs, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
        if width != 2 or fs < 2 * SHELF_FREQ:
            return wav_bytes
        pcm = np.frombuffer(w.readframes(n), dtype="<i2").reshape(-1, ch).astype(np.float64) / 32768.0
    h = _impulse(fs)
    out = np.stack([_convolve(pcm[:, c], h) for c in range(ch)], axis=1)
    peak = float(np.abs(out).max()) if out.size else 0.0
    if peak > PEAK:
        out *= PEAK / peak
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(ch)
        w.setsampwidth(2)
        w.setframerate(fs)
        w.writeframes(np.clip(np.round(out * 32767.0), -32768, 32767).astype("<i2").tobytes())
    return buf.getvalue()
