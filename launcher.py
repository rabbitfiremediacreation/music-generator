#!/usr/bin/env python3
"""Music Generator – Schlanker, rahmenloser Desktop-Launcher für Arch Linux.

Autor: ZeroDot1
Verwendet ausschließlich die Python-Standardbibliothek (tkinter) ohne externe Abhängigkeiten.
Stellt alle Funktionen des Skripts arch_linux.sh grafisch bereit.
"""
import os
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path
import tkinter as tk
from tkinter import ttk, messagebox

ROOT_DIR = Path(__file__).resolve().parent
ARCH_SCRIPT = ROOT_DIR / "arch_linux.sh"

# Dunkles Theme Farbpalette (Modern Dark)
BG_DARK = "#18181b"          # Zinc 900
BG_PANEL = "#27272a"         # Zinc 800
BG_CARD = "#3f3f46"          # Zinc 700
BORDER_COLOR = "#52525b"     # Zinc 600
TEXT_MAIN = "#f4f4f5"        # Zinc 100
TEXT_MUTED = "#a1a1aa"       # Zinc 400
ACCENT_GREEN = "#10b981"     # Emerald 500
ACCENT_BLUE = "#3b82f6"      # Blue 500
ACCENT_ORANGE = "#f59e0b"    # Amber 500
ACCENT_RED = "#ef4444"       # Red 500
BTN_BG = "#2e2e33"           # Dark button base
BTN_HOVER = "#3f3f46"        # Button hover


class FramelessLauncher(tk.Tk):
    def __init__(self) -> None:
        super().__init__()

        self.title("Music Generator Launcher")
        # Rahmenloses Fenster
        self.overrideredirect(True)
        self.configure(bg=BORDER_COLOR)

        # Standard-Geometrie & Zentrieren
        self.width = 620
        self.height = 680
        screen_w = self.winfo_screenwidth()
        screen_h = self.winfo_screenheight()
        pos_x = (screen_w - self.width) // 2
        pos_y = (screen_h - self.height) // 2
        self.geometry(f"{self.width}x{self.height}+{pos_x}+{pos_y}")

        # Drag-Handling für rahmenloses Fenster
        self._drag_start_x = 0
        self._drag_start_y = 0

        # Aktiver Prozess
        self.running_proc = None

        self._build_ui()
        self._check_status_async()

    def _build_ui(self) -> None:
        # Äußerer Container mit dezentem Rahmen
        self.main_frame = tk.Frame(self, bg=BG_DARK, bd=0)
        self.main_frame.pack(fill=tk.BOTH, expand=True, padx=1, pady=1)

        # --- 1. Custom Title Bar (zum Ziehen & Schließen) ---
        self.title_bar = tk.Frame(self.main_frame, bg=BG_PANEL, height=38, bd=0)
        self.title_bar.pack(fill=tk.X, side=tk.TOP)
        self.title_bar.pack_propagate(False)

        self.title_bar.bind("<ButtonPress-1>", self._start_drag)
        self.title_bar.bind("<B1-Motion>", self._do_drag)

        title_lbl = tk.Label(
            self.title_bar,
            text="🎵 Music Generator · Arch Linux Launcher",
            fg=TEXT_MAIN,
            bg=BG_PANEL,
            font=("Cantarell", 11, "bold"),
        )
        title_lbl.pack(side=tk.LEFT, padx=12)
        title_lbl.bind("<ButtonPress-1>", self._start_drag)
        title_lbl.bind("<B1-Motion>", self._do_drag)

        # Schließen- und Minimieren-Buttons
        btn_close = tk.Label(
            self.title_bar,
            text="✕",
            fg=TEXT_MUTED,
            bg=BG_PANEL,
            font=("Cantarell", 12),
            cursor="hand2",
            padx=12,
        )
        btn_close.pack(side=tk.RIGHT, fill=tk.Y)
        btn_close.bind("<Enter>", lambda e: btn_close.configure(bg=ACCENT_RED, fg="#ffffff"))
        btn_close.bind("<Leave>", lambda e: btn_close.configure(bg=BG_PANEL, fg=TEXT_MUTED))
        btn_close.bind("<Button-1>", lambda e: self.destroy())

        btn_min = tk.Label(
            self.title_bar,
            text="—",
            fg=TEXT_MUTED,
            bg=BG_PANEL,
            font=("Cantarell", 10),
            cursor="hand2",
            padx=10,
        )
        btn_min.pack(side=tk.RIGHT, fill=tk.Y)
        btn_min.bind("<Enter>", lambda e: btn_min.configure(bg=BG_CARD, fg="#ffffff"))
        btn_min.bind("<Leave>", lambda e: btn_min.configure(bg=BG_PANEL, fg=TEXT_MUTED))
        btn_min.bind("<Button-1>", lambda e: self._minimize())

        # --- 2. Kopfbereich & Status ---
        head_box = tk.Frame(self.main_frame, bg=BG_DARK, pady=10, padx=16)
        head_box.pack(fill=tk.X)

        author_lbl = tk.Label(
            head_box,
            text="Autor: ZeroDot1 · C++ GGUF Engine · ROCm (RX 9070 XT) / Vulkan / CUDA",
            fg=TEXT_MUTED,
            bg=BG_DARK,
            font=("Cantarell", 9),
        )
        author_lbl.pack(anchor="w")

        status_box = tk.Frame(head_box, bg=BG_PANEL, padx=10, pady=8, highlightbackground=BORDER_COLOR, highlightthickness=1)
        status_box.pack(fill=tk.X, pady=6)

        self.status_indicator = tk.Label(status_box, text="●", fg=TEXT_MUTED, bg=BG_PANEL, font=("Cantarell", 12))
        self.status_indicator.pack(side=tk.LEFT, padx=(2, 6))

        self.status_text = tk.Label(
            status_box,
            text="Prüfe Server-Status...",
            fg=TEXT_MAIN,
            bg=BG_PANEL,
            font=("Cantarell", 10),
        )
        self.status_text.pack(side=tk.LEFT)

        btn_refresh = tk.Label(
            status_box,
            text="🔄 Aktualisieren",
            fg=ACCENT_BLUE,
            bg=BG_PANEL,
            font=("Cantarell", 9, "underline"),
            cursor="hand2",
        )
        btn_refresh.pack(side=tk.RIGHT, padx=4)
        btn_refresh.bind("<Button-1>", lambda e: self._check_status_async())

        # --- 3. Optionen (Backend, LM, Flags) ---
        opt_card = tk.LabelFrame(
            self.main_frame,
            text=" Installation & Start-Optionen ",
            fg=TEXT_MAIN,
            bg=BG_DARK,
            font=("Cantarell", 10, "bold"),
            padx=12,
            pady=8,
            highlightbackground=BORDER_COLOR,
            highlightthickness=1,
            bd=0,
        )
        opt_card.pack(fill=tk.X, padx=16, pady=4)

        # Backend Selector
        f1 = tk.Frame(opt_card, bg=BG_DARK)
        f1.pack(fill=tk.X, pady=2)
        tk.Label(f1, text="Hardware-Backend:", fg=TEXT_MUTED, bg=BG_DARK, width=18, anchor="w").pack(side=tk.LEFT)
        self.var_backend = tk.StringVar(value="auto")
        for b in ["auto", "rocm", "vulkan", "cuda", "cpu"]:
            r = tk.Radiobutton(
                f1, text=b, value=b, variable=self.var_backend,
                fg=TEXT_MAIN, bg=BG_DARK, activebackground=BG_DARK, selectcolor=BG_PANEL,
                highlightthickness=0, font=("Cantarell", 9),
            )
            r.pack(side=tk.LEFT, padx=4)

        # LM Größe Selector
        f2 = tk.Frame(opt_card, bg=BG_DARK)
        f2.pack(fill=tk.X, pady=2)
        tk.Label(f2, text="Sprachmodell (LM):", fg=TEXT_MUTED, bg=BG_DARK, width=18, anchor="w").pack(side=tk.LEFT)
        self.var_lm = tk.StringVar(value="Standard (Auto)")
        for lm in ["Auto", "0.6B", "1.7B", "4B"]:
            r = tk.Radiobutton(
                f2, text=lm, value=lm, variable=self.var_lm,
                fg=TEXT_MAIN, bg=BG_DARK, activebackground=BG_DARK, selectcolor=BG_PANEL,
                highlightthickness=0, font=("Cantarell", 9),
            )
            r.pack(side=tk.LEFT, padx=4)

        # Checkboxen (yay, XL, no-browser)
        f3 = tk.Frame(opt_card, bg=BG_DARK)
        f3.pack(fill=tk.X, pady=4)
        self.var_yay = tk.BooleanVar(value=True)
        self.var_xl = tk.BooleanVar(value=False)
        self.var_nobrowser = tk.BooleanVar(value=False)

        cb1 = tk.Checkbutton(
            f3, text="AUR Pakete via yay", variable=self.var_yay,
            fg=TEXT_MAIN, bg=BG_DARK, activebackground=BG_DARK, selectcolor=BG_PANEL,
            highlightthickness=0, font=("Cantarell", 9),
        )
        cb1.pack(side=tk.LEFT, padx=(0, 10))

        cb2 = tk.Checkbutton(
            f3, text="XL Modell (5.3 GB)", variable=self.var_xl,
            fg=TEXT_MAIN, bg=BG_DARK, activebackground=BG_DARK, selectcolor=BG_PANEL,
            highlightthickness=0, font=("Cantarell", 9),
        )
        cb2.pack(side=tk.LEFT, padx=10)

        # --- 4. Aktions-Buttons ---
        btn_box = tk.Frame(self.main_frame, bg=BG_DARK, padx=16, pady=6)
        btn_box.pack(fill=tk.X)

        self._create_button(btn_box, "🚀 Starten & Öffnen", ACCENT_GREEN, self._action_start, row=0, col=0)
        self._create_button(btn_box, "⏹ Server Stoppen", ACCENT_RED, self._action_stop, row=0, col=1)
        self._create_button(btn_box, "📦 Nur Installieren / Bauen", ACCENT_BLUE, self._action_install, row=1, col=0)
        self._create_button(btn_box, "🔨 Engine Neu Kompilieren", ACCENT_ORANGE, self._action_rebuild, row=1, col=1)
        self._create_button(btn_box, "🌐 Im Browser Öffnen", BTN_BG, self._action_browser, row=2, col=0)
        self._create_button(btn_box, "🗑 App Deinstallieren", "#71717a", self._action_uninstall, row=2, col=1)

        # --- 5. Terminal-Ausgabe / Log Console ---
        log_card = tk.Frame(self.main_frame, bg=BG_DARK, padx=16, pady=4)
        log_card.pack(fill=tk.BOTH, expand=True)

        lbl_log = tk.Label(log_card, text="Ausgabe-Log:", fg=TEXT_MUTED, bg=BG_DARK, font=("Cantarell", 9))
        lbl_log.pack(anchor="w")

        self.log_text = tk.Text(
            log_card,
            bg="#111113",
            fg="#e4e4e7",
            insertbackground="#ffffff",
            bd=0,
            font=("Monospace", 9),
            wrap=tk.WORD,
            highlightbackground=BORDER_COLOR,
            highlightthickness=1,
        )
        self.log_text.pack(fill=tk.BOTH, expand=True, pady=4)

        # Footer
        footer = tk.Frame(self.main_frame, bg=BG_PANEL, height=24)
        footer.pack(fill=tk.X, side=tk.BOTTOM)
        footer_lbl = tk.Label(footer, text="Music Generator · Lokales KI-Musikstudio", fg=TEXT_MUTED, bg=BG_PANEL, font=("Cantarell", 8))
        footer_lbl.pack(side=tk.LEFT, padx=8)

    def _create_button(self, parent, text, color, cmd, row, col) -> None:
        btn = tk.Button(
            parent,
            text=text,
            bg=color,
            fg="#ffffff" if color != TEXT_MAIN else "#000000",
            activebackground=BORDER_COLOR,
            activeforeground="#ffffff",
            font=("Cantarell", 9, "bold"),
            bd=0,
            padx=10,
            pady=6,
            cursor="hand2",
            relief=tk.FLAT,
            command=cmd,
        )
        btn.grid(row=row, column=col, sticky="ew", padx=4, pady=3)
        parent.grid_columnconfigure(col, weight=1)

    def _start_drag(self, event) -> None:
        self._drag_start_x = event.x
        self._drag_start_y = event.y

    def _do_drag(self, event) -> None:
        deltax = event.x - self._drag_start_x
        deltay = event.y - self._drag_start_y
        new_x = self.winfo_x() + deltax
        new_y = self.winfo_y() + deltay
        self.geometry(f"+{new_x}+{new_y}")

    def _minimize(self) -> None:
        self.withdraw()
        self.after(200, self.deiconify)

    def log(self, text: str) -> None:
        self.log_text.insert(tk.END, text + "\n")
        self.log_text.see(tk.END)

    def _get_common_args(self) -> list[str]:
        args = []
        if self.var_yay.get():
            args.append("--yay")
        backend = self.var_backend.get()
        if backend != "auto":
            args.extend(["--backend", backend])
        lm = self.var_lm.get()
        if lm in ["0.6B", "1.7B", "4B"]:
            args.extend(["--lm", lm])
        if self.var_xl.get():
            args.append("--xl")
        return args

    def _run_script_async(self, script_args: list[str]) -> None:
        if self.running_proc and self.running_proc.poll() is None:
            self.log("⚠️ Ein Vorgang läuft bereits. Bitte warten...")
            return

        def worker():
            cmd = [str(ARCH_SCRIPT)] + script_args
            self.log(f"▶ Starte: {' '.join(cmd)}")
            try:
                self.running_proc = subprocess.Popen(
                    cmd,
                    cwd=str(ROOT_DIR),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    bufsize=1,
                )
                for line in self.running_proc.stdout:
                    self.log_text.insert(tk.END, line)
                    self.log_text.see(tk.END)
                self.running_proc.wait()
                rc = self.running_proc.returncode
                if rc == 0:
                    self.log("✓ Vorgang erfolgreich abgeschlossen.")
                else:
                    self.log(f"✗ Vorgang mit Exit-Code {rc} beendet.")
            except Exception as e:
                self.log(f"✗ Fehler bei der Ausführung: {e}")
            finally:
                self._check_status_async()

        threading.Thread(target=worker, daemon=True).start()

    def _check_status_async(self) -> None:
        def worker():
            try:
                res = subprocess.run(
                    [str(ARCH_SCRIPT), "--status"],
                    cwd=str(ROOT_DIR),
                    capture_output=True,
                    text=True,
                )
                output = res.stdout
                is_running = "✓ Web app running" in output
                if is_running:
                    self.status_indicator.configure(text="●", fg=ACCENT_GREEN)
                    self.status_text.configure(text="Server läuft · http://localhost:8765")
                else:
                    self.status_indicator.configure(text="○", fg=TEXT_MUTED)
                    self.status_text.configure(text="Server gestoppt (bereit zum Start)")
            except Exception:
                self.status_indicator.configure(text="?", fg=ACCENT_RED)
                self.status_text.configure(text="Status konnte nicht ermittelt werden")

        threading.Thread(target=worker, daemon=True).start()

    def _action_start(self) -> None:
        args = ["--start"] + self._get_common_args()
        self._run_script_async(args)

    def _action_stop(self) -> None:
        self._run_script_async(["--stop"])

    def _action_install(self) -> None:
        args = ["--install-only"] + self._get_common_args()
        self._run_script_async(args)

    def _action_rebuild(self) -> None:
        args = ["--rebuild", "--install-only"] + self._get_common_args()
        self._run_script_async(args)

    def _action_browser(self) -> None:
        import webbrowser
        webbrowser.open("http://localhost:8765")
        self.log("🌐 Öffne http://localhost:8765 im Standardbrowser...")

    def _action_uninstall(self) -> None:
        if messagebox.askyesno(
            "Music Generator Deinstallation",
            "Möchtest du Music Generator deinstallieren?\n\n"
            "Dies entfernt die kompilierten Binaries, Modelle und lokalen Daten.\n"
            "Systempakete in Arch Linux bleiben erhalten.",
        ):
            self._run_script_async(["--uninstall", "--yes"])


def main() -> None:
    app = FramelessLauncher()
    app.mainloop()


if __name__ == "__main__":
    main()
