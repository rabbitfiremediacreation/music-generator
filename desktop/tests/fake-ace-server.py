#!/usr/bin/env python3
"""Ersatz für ace-server in Tests: beantwortet /health und /props, ohne Modelle zu laden.

Versteht dieselben Startoptionen (--host, --port, --models, --max-batch, --vae-chunk, --device). Mit der
Umgebungsvariable FAKE_ACE_FAIL=1 beendet er sich sofort mit Fehler (Absturz-Test), mit FAKE_ACE_DELAY=<s> wartet er
vor dem Öffnen des Ports.
"""
import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

args = sys.argv[1:]
if "--help" in args:
    print("ace-server (fake) usage: --host H --port P --models DIR [--max-batch N] [--vae-chunk N] [--device DEV]")
    sys.exit(0)
if os.environ.get("FAKE_ACE_FAIL") == "1":
    print("fake ace-server: absichtlicher Fehler", file=sys.stderr)
    sys.exit(3)
opt = {args[i]: args[i + 1] for i in range(0, len(args) - 1) if args[i].startswith("--")}
time.sleep(float(os.environ.get("FAKE_ACE_DELAY", "0")))


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/health":
            body = {"status": "ok"}
        elif path == "/props":
            body = {"fake": True, "backend": os.environ.get("GGML_BACKEND", "auto"), "args": opt}
        else:
            self.send_error(404)
            return
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *a):
        print(fmt % a, flush=True)


print(f"fake ace-server on {opt.get('--host', '127.0.0.1')}:{opt['--port']} args={opt} GGML_BACKEND={os.environ.get('GGML_BACKEND', '')}", flush=True)
ThreadingHTTPServer((opt.get("--host", "127.0.0.1"), int(opt["--port"])), Handler).serve_forever()
