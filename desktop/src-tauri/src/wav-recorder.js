// Nur unter Linux (WebKitGTK), eingebunden von lib.rs: Dessen MediaRecorder liefert je nach GStreamer-Ausstattung
// leere Aufnahmen (MP4/AAC, 0 Bytes). Ersatz mit derselben Schnittstelle, soweit die Web-App sie nutzt
// (new MediaRecorder(stream), start, stop, state, mimeType, Ereignisse dataavailable und stop): nimmt die Rohdaten
// über Web Audio auf und liefert eine WAV-Datei (Mono, 16 Bit). Die Web-App dekodiert sie wie jede Aufnahme.
(() => {
  if (!["localhost", "127.0.0.1"].includes(location.hostname) || location.port !== "__PORT__") return;
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC) return;

  class WavRecorder extends EventTarget {
    constructor(stream) {
      super();
      this.stream = stream;
      this.state = "inactive";
      this.mimeType = "audio/wav";
      this.ondataavailable = null;
      this.onstop = null;
    }
    static isTypeSupported(type) {
      return /^audio\/wav/.test(type);
    }
    start() {
      if (this.state !== "inactive") return;
      this.ctx = new AC();
      this.source = this.ctx.createMediaStreamSource(this.stream);
      // ScriptProcessor ist veraltet, aber überall vorhanden und braucht keine eigene Worklet-Datei
      this.node = this.ctx.createScriptProcessor(4096, 1, 1);
      this.chunks = [];
      this.node.onaudioprocess = (e) => {
        if (this.state === "recording") this.chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
      };
      this.source.connect(this.node);
      this.node.connect(this.ctx.destination);   // ohne Verbindung zum Ausgang ruft WebKit onaudioprocess nicht auf
      this.state = "recording";
    }
    stop() {
      if (this.state !== "recording") return;
      this.state = "inactive";
      this.node.disconnect();
      this.source.disconnect();
      const rate = this.ctx.sampleRate;
      this.ctx.close();
      const blob = new Blob([wav(this.chunks, rate)], { type: this.mimeType });
      const data = new Event("dataavailable");
      data.data = blob;
      this.dispatchEvent(data);
      this.ondataavailable?.(data);
      const stop = new Event("stop");
      this.dispatchEvent(stop);
      this.onstop?.(stop);
    }
  }

  function wav(chunks, rate) {
    const n = chunks.reduce((s, c) => s + c.length, 0);
    const out = new DataView(new ArrayBuffer(44 + n * 2));
    const str = (o, v) => { for (let i = 0; i < v.length; i++) out.setUint8(o + i, v.charCodeAt(i)); };
    str(0, "RIFF"); out.setUint32(4, 36 + n * 2, true); str(8, "WAVE"); str(12, "fmt "); out.setUint32(16, 16, true);
    out.setUint16(20, 1, true); out.setUint16(22, 1, true); out.setUint32(24, rate, true);
    out.setUint32(28, rate * 2, true); out.setUint16(32, 2, true); out.setUint16(34, 16, true);
    str(36, "data"); out.setUint32(40, n * 2, true);
    let o = 44;
    for (const c of chunks) for (let i = 0; i < c.length; i++, o += 2) {
      const v = Math.max(-1, Math.min(1, c[i]));
      out.setInt16(o, v < 0 ? v * 32768 : v * 32767, true);
    }
    return out.buffer;
  }

  window.MediaRecorder = WavRecorder;
})();
