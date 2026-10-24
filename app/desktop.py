"""Betrieb in der Desktop-App (Umgebungsvariable MG_DESKTOP=1).

Die App startet diese Web-App als Kindprozess und überwacht ihn. Statt OFF-Skript oder eigenem Neustart hinterlässt
die Web-App in data/desktop-exit, was nach ihrem Ende passieren soll, und beendet sich: „quit“ = App samt
Modellservern beenden, „restart“ = nur die Web-App neu starten (nach einem Update).
"""
import asyncio
import os
import signal

from . import db

ACTIVE = os.environ.get("MG_DESKTOP") == "1"
# Installierte Desktop-App: Programmcode liegt getrennt von den Daten (MG_HOME) und wird mit der App aktualisiert, nicht per Git.
BUNDLED = ACTIVE and db.HOME != db.ROOT
EXIT_FILE = db.DATA / "desktop-exit"


def exit_now(action: str) -> None:
    db.DATA.mkdir(parents=True, exist_ok=True)
    EXIT_FILE.write_text(action)
    os.kill(os.getpid(), signal.SIGTERM)


def request_exit(action: str, delay: float = 0.5) -> None:
    """Leicht verzögert, damit die laufende Antwort noch ankommt."""
    asyncio.get_running_loop().call_later(delay, exit_now, action)
