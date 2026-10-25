"""Local smoke test: plays a quiet generated tone and captures only its afplay PID.
No microphone, network or saved captured audio. May prompt for audio permission.
Run from this directory: python3 test-tone.py
"""
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import wave

root = Path(__file__).resolve().parent
subprocess.run(["cargo", "build", "--manifest-path", str(root / "Cargo.toml")], check=True)
binary = root / "target/debug/macos-process-audio-spike"
with tempfile.TemporaryDirectory(prefix="live-translator-tone-") as directory:
    path = os.path.join(directory, "tone.wav")
    with wave.open(path, "wb") as audio:
        audio.setparams((1, 2, 48000, 0, "NONE", "not compressed"))
        audio.writeframes(b"".join(
            struct.pack("<h", int(0.015 * 32767 * math.sin(2 * math.pi * 440 * i / 48000)))
            for i in range(48000 * 20)
        ))
    player = subprocess.Popen(["/usr/bin/afplay", path])
    try:
        time.sleep(1)
        for mute in (False, True):
            print(f"TEST mute-original={mute}, owned afplay PID={player.pid}", flush=True)
            subprocess.run(
                [str(binary), "capture", str(player.pid), "3"]
                + (["--mute-original"] if mute else []),
                check=True, timeout=15,
            )
    finally:
        if player.poll() is None:
            player.terminate()
        player.wait()
