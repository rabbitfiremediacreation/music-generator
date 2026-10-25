/**
 * @file        audio-tool.js
 * @description StudioScript
 * @author      JT Farrell
 * @copyright   Copyright (c) 2024 JT Farrell
 * @license     See LICENSE-STUDIO-SCRIPT.md
 * @contact     jt.farrell@yahoo.com
 * @version     1.5.7
 *
 * This script is licensed separately from the host application.
 * Authorized use is limited to [ALPHATESTER].
 * Modifications and use in other applications require
 * prior written permission, subject to applicable law.
 */

function setStatus(text) {
	const el = document.getElementById("status");
	if (el) el.textContent = text;
}

function setLimiterStatus(text, state) {
	const el =
		document.getElementById(
			"limiterStatus"
		);

	if (!el) return;
	el.textContent = text;
	el.classList.remove(
		"active",
		"limiting",
		"clip"
	);

	if (state)
		el.classList.add(state);
}
let peakIndicator;
let audioContext;
let buffer;
let source;
let originalBuffer;
let gainNode;
let track1Gain;
let micGain;
let micMonitorGain;
let micMuted = false;
let track1Source = "audio";
let track1ConnectedSource = "audio";
let track1Muted = false;
let micSolo = false;
let track1Solo = false;
let splitterNode;
let leftAnalyser;
let rightAnalyser;
let analyserNode;
let activeAnalyser;
let limiterNode;
let masterLimiterNode;
let softClipperNode;
let softClipperEnabled = false;
let eqLow;
let eqLowMid;
let eqMid;
let eqHigh;
let eqHighMid;
let distortionPreFilter;
let convolverNode;
let dryGain;
let wetGain;
let masterGain;
let limiterThreshold = -6;
let limiterCeiling = -0.3;
let delayNode;
let delayFeedback;
let delayWetGain;
let filterFX;
let fxInputGain;
let fxOutputGain;
let delayEnabled = false;
let distortionEnabled = false;
let filterEnabled = false;
let reverbEnabled = false;
let distortionNode;
let distortionFilter;
let distortionGain;
let mediaRecorder;
let recordedChunks = [];
let recordingStream;
let recordingDestination;
let micDestination;
let drumDestination;
let masterDestination;
let micStream;
let micSource;
let micInputGain;
let micAnalyser;
let track2VuStarted = false;
let micCompressor;
let micGateGain;
let micGateData;
let micGateRunning = false;
let micGateOpen = false;
let micGateThreshold = 0.01;
let micCompAmount = 3;
let micGateState = false;
let micAmbienceGain;
let micAmbienceAmount = 0;

const loopSources = [

	"mic",      // L1
	"drums",    // L2
	"midi",   // L3
	"master",    // L4

];
let startTime = 0;
let animationId = null;
let spectrumAnimationId = null;

function createReverbImpulse(duration = 1.5) {
	if (!audioContext) return null;
	const sampleRate =
		audioContext.sampleRate;
	const length =
		sampleRate * duration;
	const impulse =
		audioContext.createBuffer(
			2,
			length,
			sampleRate
		);

	for (let ch = 0; ch < 2; ch++) {
		const data =
			impulse.getChannelData(ch);
		for (let i = 0; i < length; i++) {
			data[i] = 0.5 * Math.sin(2 * Math.PI * i / length) * Math.pow(1 - i / length, 2);
		}
	}

	return impulse;
}

let midiAccess;
let midiInput;

async function initMIDI() {
	if (!navigator.requestMIDIAccess) {
		document.getElementById("midiStatus").textContent =
			"MIDI not supported";
		return;
	}

	midiAccess = await navigator.requestMIDIAccess();
	const select = document.getElementById("midiDevice");
	select.innerHTML = "";
	midiAccess.inputs.forEach((input) => {
		const option = document.createElement("option");
		option.value = input.id;
		option.textContent = input.name;
		select.appendChild(option);
	});
	select.addEventListener("change", () => {
		const id = select.value;
		midiInput = midiAccess.inputs.get(id);
		if (midiInput) {
			midiInput.onmidimessage = handleMIDI;
			document.getElementById("midiStatus").textContent =
				"MIDI connected";
		}
	});
}

function handleMIDI(event) {
	const data = event.data;
	const status = data[0];
	const note = data[1];
	const velocity = data[2];
	if (status === 144 && velocity > 0) {
		playInstrument(note, velocity);
	}
}

function playInstrument(note, velocity) {

	const vel =
		Math.max(
			0,
			Math.min(
				1,
				velocity / 127
			)
		);

switch (note) {

	// Kick
	case 36:
		playDrum("kick", vel);
		break;

	// Snare
	case 38:
		playDrum("snare", vel);
		break;

	// Closed Hat
	case 42:
		playDrum("hihat", vel);
		break;

	// Open Hat
	case 46:
		playDrum("hihat", vel * 1.1);
		break;

	// Clap
	case 39:
		playDrum("clap", vel);
		break;

	// Tom
	case 45:
		playDrum("tom", vel);
		break;

	// Crash
	case 49:
		playDrum("crash", vel);
		break;

	// Bell
	case 53:
		playDrum("bell", vel);
		break;
}
}

initMIDI();

function unlockAudioContext() {
	if (!audioContext) return;
	if (audioContext.state === "suspended") {
		audioContext.resume();
	}
}

function updateTrackRouting() {

	switch (track1Source) {

		case "audio":

			track1ConnectedSource =
				"audio";

			break;

		case "midi":

			track1ConnectedSource =
				"midi";

			break;

		case "drum":

			track1ConnectedSource =
				"drum";

			break;

		case "looper":

			track1ConnectedSource =
				"looper";

			break;

		default:

			track1ConnectedSource =
				"audio";

	}

}

function initAudioTool() {
	if (window.audioToolInitialized) {
		return;
	}
	window.audioToolInitialized = true;

	document.body.addEventListener("click", unlockAudioContext, {
		once: true
	});
	document.body.addEventListener("touchstart", unlockAudioContext, {
		once: true
	});
	peakIndicator =
		document.getElementById("peakIndicator");

	const fileInput =
		document.getElementById("fileInput");
	if (!fileInput) {
		setTimeout(
			initAudioTool,
			300
		);

		return;
	}

	fileInput.addEventListener(
		"change",
		async function() {

			await initAudio();
			setStatus("File selected");
			const file =
				this.files[0];
			if (!file) {
				setStatus("No file");
				return;
			}
			setStatus(
				"Load file: " +
				file.name
			);
			const arrayBuffer =
				await file.arrayBuffer();
			buffer = await new Promise((resolve, reject) => {
				audioContext.decodeAudioData(
					arrayBuffer.slice(0),
					resolve,
					reject
				);
			});

			originalBuffer = audioContext.createBuffer(
				buffer.numberOfChannels,
				buffer.length,
				buffer.sampleRate
			);

			for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
				originalBuffer.getChannelData(ch).set(
					buffer.getChannelData(ch)
				);
			}
			setStatus("Audio loaded");
			drawWaveform();
			this.value = "";
		}
	);

	document
		.getElementById("eqLow")
		?.addEventListener(
			"input",
			function() {
				if (!eqLow) return;
				eqLow.gain.value =
					parseFloat(
						this.value
					);
			}
		);

	document
		.getElementById("eqLowMid")
		?.addEventListener(
			"input",
			e => {
				if (!eqLowMid)
					return;
				eqLowMid.gain.value =
					parseFloat(
						e.target.value
					);
			}
		);

	document
		.getElementById("eqMid")
		?.addEventListener(
			"input",
			function() {
				if (!eqMid) return;
				eqMid.gain.value =
					parseFloat(
						this.value
					);
			}
		);

	document
		.getElementById("eqHigh")
		?.addEventListener(
			"input",
			function() {
				if (!eqHigh) return;
				eqHigh.gain.value =
					parseFloat(
						this.value
					);
			}
		);

	document
		.getElementById("eqHighMid")
		?.addEventListener(
			"input",
			e => {
				if (!eqHighMid)
					return;
				eqHighMid.gain.value =
					parseFloat(
						e.target.value
					);
			}
		);

	document
		.getElementById("eqPreset")
		?.addEventListener(
			"change",
			e => {
				const preset =
					e.target.value;
				if (preset === "flat") {
					setEQ(
						0,
						0,
						0,
						0,
						0
					);
				}

				if (preset === "bass") {
					setEQ(
						6,
						3,
						0,
						2,
						2
					);

				}

				if (preset === "vocal") {
					setEQ(
						-2,
						2,
						4,
						3,
						2
					);

				}

				if (preset === "podcast") {
					setEQ(
						-3,
						1,
						5,
						3,
						1
					);

				}

				if (preset === "master") {
					setEQ(
						1,
						1,
						2,
						2,
						1
					);

				}

			}
		);

	function setEQ(
		low,
		lowMid,
		mid,
		highMid,
		high
	) {

		document
			.getElementById("eqLow")
			.value = low;
		document
			.getElementById("eqLowMid")
			.value = lowMid;
		document
			.getElementById("eqMid")
			.value = mid;
		document
			.getElementById("eqHighMid")
			.value = highMid;
		document
			.getElementById("eqHigh")
			.value = high;
		if (eqLow)
			eqLow.gain.value = low;
		if (eqLowMid)
			eqLowMid.gain.value = lowMid;
		if (eqMid)
			eqMid.gain.value = mid;
		if (eqHighMid)
			eqHighMid.gain.value = highMid;
		if (eqHigh)
			eqHigh.gain.value = high;
	}

	document
		.getElementById(
			"reverbToggle"
		)
		?.addEventListener(
			"click",
			function() {

				if (!wetGain) return;
				reverbEnabled = !reverbEnabled;

				wetGain.gain.value =
					reverbEnabled ?
					0.753 :
					0;
				this.textContent =
					reverbEnabled ?
					"Reverb ON" :
					"Reverb OFF";
				setStatus(
					reverbEnabled ?
					"Reverb enabled" :
					"Reverb disabled"
				);
			}
		);

	document
		.getElementById(
			"reverbSize"
		)
		?.addEventListener(
			"input",
			function() {
				if (!convolverNode)
					return;
				const size =
					parseFloat(
						this.value
					);
				convolverNode.buffer =
					createReverbImpulse(
						size
					);
			}
		);

	document
	.getElementById(
		"reverbWet"
	)
	?.addEventListener(
		"input",
		function() {

			if (!wetGain)
				return;

			if (!reverbEnabled)
				return;

			wetGain.gain.value =
				parseFloat(
					this.value
				);
		}
	);

	document
		.getElementById("delayToggle")
		?.addEventListener(
			"click",
			function() {

				delayEnabled = !delayEnabled;
				delayWetGain.gain.value =
					delayEnabled ?
					0.35 :
					0;
				this.textContent =
					delayEnabled ?
					"Delay ON" :
					"Delay OFF";
			}
		);

	document
		.getElementById("delayTime")
		?.addEventListener(
			"input",
			function() {
				if (!delayNode) return;
				delayNode.delayTime.value =
					parseFloat(this.value);
			}
		);

	document
		.getElementById("delayFeedback")
		?.addEventListener(
			"input",
			function() {
				if (!delayFeedback) return;
				delayFeedback.gain.value =
					parseFloat(this.value);
			}
		);

	document
		.getElementById("distortionAmount")
		?.addEventListener(
			"input",
			function() {
				if (!distortionNode)
					return;
				const amount =
					parseFloat(this.value);
				distortionNode.curve =
					createDistortionCurve(amount);
			}
		);

	document
		.getElementById("filterFreq")
		?.addEventListener(
			"input",
			function() {
				if (!filterFX)
					return;
				filterFX.frequency.value =
					parseFloat(this.value);
			}
		);

	document
		.getElementById(
			"distortionToggle"
		)
		?.addEventListener(
			"click",
			function() {
				if (!distortionNode)
					return;
				distortionEnabled = !distortionEnabled;
				let drive =
	parseFloat(
		document.getElementById(
			"distortionDrive"
		).value
	);
				distortionGain.gain.value =
					distortionEnabled ? 0.45 : 0;
				distortionNode.curve =
					createDistortionCurve(drive);
				this.textContent =
					distortionEnabled ?
					"Distortion ON" :
					"Distortion OFF";
				setStatus(
					distortionEnabled ?
					"Distortion enabled" :
					"Distortion disabled"
				);

				updateDistortionConnection();
			}
		);

	document
		.getElementById(
			"distortionDrive"
		)
		?.addEventListener(
			"input",
			function() {
				if (
					!distortionNode ||
					!distortionEnabled
				)
					return;

				let amount =
					parseFloat(this.value);
				amount = Math.max(
					0,
					Math.min(amount, 12)
				);

				distortionNode.curve =
					createDistortionCurve(
						amount
					);
			}
		);

	document
		.getElementById(
			"distortionTone"
		)
		?.addEventListener(
			"input",
			function() {
				if (!distortionFilter)
					return;
				let freq =
					parseFloat(this.value);
				freq = Math.max(
					400,
					Math.min(12000, freq)
				);
				distortionFilter.frequency.value =
					freq;
			}
		);
	document
		.getElementById(
			"recordStart"
		)
		?.addEventListener(
			"click",
			startRecording
		);
	document
		.getElementById(
			"recordStop"
		)
		?.addEventListener(
			"click",
			stopRecording
		);
	document
		.getElementById(
			"recordDownload"
		)
		?.addEventListener(
			"click",
			downloadRecording
		);
	document
		.getElementById(
			"themeToggle"
		)
		?.addEventListener(
			"click",
			() => {
				document.body.classList.toggle(
					"light"
				);
			}
		);
	document
		.getElementById("play")
		?.addEventListener(
			"click",
			playAudio
		);
	document
		.getElementById("stop")
		?.addEventListener(
			"click",
			stopAudio
		);
	document
	.getElementById("volume")
	?.addEventListener(
		"input",
		function() {

			if (!masterGain)
				return;

			const value =
				parseFloat(
					this.value
				);

			masterGain.gain.cancelScheduledValues(
				audioContext.currentTime
			);

			masterGain.gain.setValueAtTime(
				value,
				audioContext.currentTime
			);

			setStatus(
				"Master: " +
				value.toFixed(1)
			);

		}
	);
	document
		.getElementById(
			"louder"
		)
		?.addEventListener(
			"click",
			() => {
				changeVolume(1.2);
			}
		);
	document
		.getElementById(
			"quieter"
		)
		?.addEventListener(
			"click",
			() => {
				changeVolume(0.8);
			}
		);
	document
		.getElementById(
			"fadeIn"
		)
		?.addEventListener(
			"click",
			fadeInAudio
		);
	document
		.getElementById(
			"fadeOut"
		)
		?.addEventListener(
			"click",
			fadeOutAudio
		);
	document
		.getElementById(
			"clean"
		)
		?.addEventListener(
			"click",
			cleanAudio
		);
	document
		.getElementById(
			"download"
		)
		?.addEventListener(
			"click",
			downloadAudio
		);
	document
		.getElementById(
			"master"
		)
		?.addEventListener(
			"click",
			masteringLight
		);
	window.addEventListener(
		"resize",
		() => {
			if (buffer)
				drawWaveform();
		}
	);

	document
		.getElementById(
			"exportPreset"
		)
		?.addEventListener(
			"change",
			function() {
				const preset =
					this.value;
				const bitDepth =
					document.getElementById(
						"bitDepth"
					);
				const sampleRate =
					document.getElementById(
						"sampleRate"
					);
				if (!bitDepth || !sampleRate)
					return;
				switch (preset) {
					case "cd":
						bitDepth.value = "16";
						sampleRate.value = "44100";
						break;
					case "stream":
						bitDepth.value = "24";
						sampleRate.value = "44100";
						break;
					case "master":
						bitDepth.value = "24";
						sampleRate.value = "48000";
						break;
					default:
						break;
				}

			}
		);

	document
		.getElementById("drumStart")
		?.addEventListener(
			"click",
			startDrumMachine
		);
	document
		.getElementById("drumStop")
		?.addEventListener(
			"click",
			stopDrumMachine
		);
	document
		.getElementById("drumBpm")
		?.addEventListener(
			"input",
			e => {
				drumMachine.bpm =
					parseInt(
						e.target.value
					) || 120;

				if (
					drumMachine.playing
				) {
					stopDrumMachine();
					startDrumMachine();
				}
			}
		);

	document
		.getElementById("drumSwing")
		?.addEventListener(
			"input",
			e => {
				drumMachine.swing =
					parseFloat(
						e.target.value
					) || 0;
			}
		);

	document
		.getElementById("drumHumanize")
		?.addEventListener(
			"input",
			e => {
				drumMachine.humanize =
					parseFloat(
						e.target.value
					) || 0;
			}
		);

    const track1SourceSelect =
	document.getElementById(
		"track1Source"
	);

if (track1SourceSelect) {

	track1SourceSelect.addEventListener(
		"change",
		function() {

			track1Source =
	this.value;

updateTrackRouting();

drawWaveform();

		}
	);

}

}


function updateDistortionConnection() {
	if (
		!filterFX ||
		!distortionGain ||
		!distortionPreFilter ||
		!distortionNode ||
		!distortionFilter ||
		!fxOutputGain
	) return;
	try {
		distortionGain.disconnect();
	} catch (e) {}
	try {
		distortionPreFilter.disconnect();
	} catch (e) {}
	try {
		distortionNode.disconnect();
	} catch (e) {}
	try {
		distortionFilter.disconnect();
	} catch (e) {}
	if (distortionEnabled) {
		filterFX.connect(distortionGain);
		distortionGain.connect(distortionPreFilter);
		distortionPreFilter.connect(distortionNode);
		distortionNode.connect(distortionFilter);
		distortionFilter.connect(fxOutputGain);
	} else {
		try {
			filterFX.disconnect();
		} catch (e) {}
		filterFX.connect(fxOutputGain);
	}
}

function createSoftClipCurve(amount = 2) {
	const samples = 44100;
	const curve =
		new Float32Array(samples);
	for (let i = 0; i < samples; i++) {
		const x =
			(i * 2 / samples) - 1;
		curve[i] =
			Math.tanh(x * amount);
	}

	return curve;
}

function createDistortionCurve(amount = 20) {
	const samples = 44100;
	const curve =
		new Float32Array(samples);
	amount = Math.max(
		0,
		Math.min(amount, 100)
	);
	const k =
		amount * 0.6;
	const deg =
		Math.PI / 180;
	for (let i = 0; i < samples; i++) {
		const x =
			(i * 2 / samples) - 1;
		curve[i] =
			((3 + k) * x * 20 * deg) /
			(Math.PI + k * Math.abs(x));
	}

	return curve;
}

async function initAudio() {
	if (!audioContext) {
		audioContext = new AudioContext();
	}
	if (audioContext.state !== "running") {
		try {
			await audioContext.resume();
		} catch (e) {
			console.error(
				"Resume failed:",
				e
			);
		}
	}
	if (!gainNode) {
		gainNode =
			audioContext
			.createGain();
		gainNode.gain.value = 1;

    track1Gain =
	audioContext.createGain();

track1Gain.gain.value = 0.75;

    micGain =
	audioContext.createGain();

micGain.gain.value = 0.60;

    micMonitorGain =
    audioContext.createGain();

micMonitorGain.gain.value = 1.0;

		analyserNode =
			audioContext
			.createAnalyser();

    track1Analyser =
    audioContext
    .createAnalyser();

track1Analyser.fftSize = 2048;
track1Analyser.smoothingTimeConstant = 0.8;

    splitterNode =
	audioContext
	.createChannelSplitter(2);

leftAnalyser =
	audioContext
	.createAnalyser();

rightAnalyser =
	audioContext
	.createAnalyser();

leftAnalyser.fftSize = 2048;
rightAnalyser.fftSize = 2048;
		softClipperNode =
			audioContext.createWaveShaper();
		softClipperNode.curve =
			createSoftClipCurve(0.75);
		softClipperNode.oversample =
			"4x";
		limiterNode =
			audioContext
			.createDynamicsCompressor();

		limiterNode.threshold.value = -6;
		limiterNode.knee.value = 0;
		limiterNode.ratio.value = 20;
		limiterNode.attack.value = 0.005;
		limiterNode.release.value = 0.15;
		analyserNode.fftSize = 2048;
analyserNode.smoothingTimeConstant = 0.8;

activeAnalyser =
	analyserNode;

    masterLimiterNode =
	audioContext
	.createDynamicsCompressor();

masterLimiterNode.threshold.value = -0.3;
masterLimiterNode.knee.value = 0;
masterLimiterNode.ratio.value = 20;
masterLimiterNode.attack.value = 0.001;
masterLimiterNode.release.value = 0.08;

		eqLow =
			audioContext
			.createBiquadFilter();
		eqLow.type =
			"lowshelf";
		eqLow.frequency.value =
			120;
		eqLowMid =
			audioContext
			.createBiquadFilter();
		eqLowMid.type =
			"peaking";
		eqLowMid.frequency.value =
			400;
		eqLowMid.Q.value =
			1;
		eqMid =
			audioContext
			.createBiquadFilter();
		eqMid.type =
			"peaking";
		eqMid.frequency.value =
			1200;

		eqMid.Q.value =
			1;
		eqHighMid =
			audioContext
			.createBiquadFilter();
		eqHighMid.type =
			"peaking";
		eqHighMid.frequency.value =
			3500;
		eqHighMid.Q.value =
			1;
		eqHigh =
			audioContext
			.createBiquadFilter();
		eqHigh.type =
			"highshelf";
		eqHigh.frequency.value =
			9000;
		convolverNode =
			audioContext
			.createConvolver();
		convolverNode.buffer =
			createReverbImpulse(1.5);
		dryGain =
			audioContext
			.createGain();
		wetGain =
			audioContext
			.createGain();
		masterGain =
			audioContext
			.createGain();
		dryGain.gain.value = 1;
		wetGain.gain.value = 0;
		masterGain.gain.value = 1.0;

		delayNode =
			audioContext.createDelay(2.0);
		delayNode.delayTime.value =
			0.35;
		delayFeedback =
			audioContext.createGain();
		delayFeedback.gain.value =
			0.35;
		delayWetGain =
			audioContext.createGain();
		delayWetGain.gain.value = 0;

		filterFX =
			audioContext.createBiquadFilter();
		filterFX.type =
			"lowpass";
		filterFX.frequency.value =
			22000;
		filterFX.Q.value = 1;

		fxInputGain =
			audioContext.createGain();

		fxOutputGain =
			audioContext.createGain();

		fxInputGain.gain.value = 1;
		fxOutputGain.gain.value = 1;

        micInputGain =
	audioContext.createGain();

micInputGain.gain.value = 1.0;

micCompressor =
	audioContext.createDynamicsCompressor();

micCompressor.threshold.value = -24;
micCompressor.knee.value = 18;
micCompressor.ratio.value =
	micCompAmount;
micCompressor.attack.value = 0.003;
micCompressor.release.value = 0.12;

micGateGain =
	audioContext.createGain();

micGateGain.gain.value = 1.0;

    micAmbienceGain =
	audioContext.createGain();

micAmbienceGain.gain.value = 0;

micAnalyser =
	audioContext.createAnalyser();

micAnalyser.fftSize = 512;
micAnalyser.smoothingTimeConstant = 0.85;

micGateData =
	new Uint8Array(
		micAnalyser.fftSize
	);

		distortionPreFilter =
			audioContext.createBiquadFilter();
		distortionPreFilter.type =
			"lowpass";
		distortionPreFilter.frequency.value =
			8500;
		distortionPreFilter.Q.value =
			0.7;
		distortionNode =
			audioContext.createWaveShaper();
		distortionNode.curve =
			createDistortionCurve(0);
		distortionNode.oversample =
			"4x";
		distortionFilter =
			audioContext.createBiquadFilter();
		distortionFilter.type =
			"lowpass";
		distortionFilter.frequency.value =
			6500;
		distortionFilter.Q.value =
			0.7;

		gainNode.connect(eqLow);
		eqLow.connect(eqLowMid);
		eqLowMid.connect(eqMid);
		eqMid.connect(eqHighMid);
		eqHighMid.connect(eqHigh);
		eqHigh.connect(limiterNode);

		limiterNode.connect(dryGain);
		limiterNode.connect(convolverNode);

		const reverbBoost =
			audioContext.createGain();

		reverbBoost.gain.value = 1.05;
		convolverNode.connect(
			reverbBoost
		);
		reverbBoost.connect(
			wetGain
		);
		dryGain.connect(fxInputGain);
		wetGain.connect(fxInputGain);

		fxInputGain.connect(filterFX);
		distortionGain =
        audioContext.createGain();
		distortionGain.gain.value = 0;
		updateDistortionConnection();
		fxOutputGain.gain.value = 0.75;
		fxOutputGain.connect(delayNode);
		delayNode.connect(delayFeedback);
		delayFeedback.connect(delayNode);
		delayNode.connect(delayWetGain);
		fxOutputGain.connect(masterGain);
		delayWetGain.connect(masterGain);
        micMonitorGain.connect(
    masterGain
);
    masterGain.connect(
    analyserNode
);

    track1Gain.connect(analyserNode);
micGain.connect(analyserNode);

track1Gain.connect(
    track1Analyser
);
		splitterNode.connect(leftAnalyser, 0);
        splitterNode.connect(rightAnalyser, 1);
        analyserNode.connect(splitterNode);
        analyserNode.connect(audioContext.destination);
        updateSoftClipRouting();
    micDestination =
	audioContext.createMediaStreamDestination();
drumDestination =
	audioContext.createMediaStreamDestination();
masterDestination =
	audioContext.createMediaStreamDestination();
		recordingDestination =
    audioContext.createMediaStreamDestination();
masterLimiterNode.connect(
    recordingDestination
);
    masterLimiterNode.connect(
	masterDestination
);

    if (!drumMachine.kick) {

	await loadSampleFromUrl(
		"kick",
		"/static/studio/samples/kick.wav"
	);

	await loadSampleFromUrl(
		"snare",
		"/static/studio/samples/snare.wav"
	);

	await loadSampleFromUrl(
		"hihat",
		"/static/studio/samples/hihat.wav"
	);

    await loadSampleFromUrl(
		"tom",
		"/static/studio/samples/tom.wav"
	);

    await loadSampleFromUrl(
		"clap",
		"/static/studio/samples/clap.wav"
	);

    await loadSampleFromUrl(
		"hat",
		"/static/studio/samples/hat.wav"
	);

    await loadSampleFromUrl(
		"bell",
		"/static/studio/samples/bell.wav"
	);

    await loadSampleFromUrl(
		"crash",
		"/static/studio/samples/crash.wav"
	);
}
	}

    if (!window.micVuStarted) {

	window.micVuStarted = true;

	updateMicVU();
}

    if (!track2VuStarted) {

	track2VuStarted = true;

	updateTrack2VU();

}

	await new Promise(resolve => setTimeout(resolve, 50));
}

async function playAudio() {
	if (!buffer) {
		setStatus("No file loaded");
		return;
	}
	await initAudio();
	if (source) {
		try {
			source.stop();
		} catch (e) {}
		source = null;
	}
	source =
		audioContext.createBufferSource();
	source.buffer =
		buffer;
    track1Source = "audio";
track1ConnectedSource = "audio";
activeAnalyser = analyserNode;

	if (track1ConnectedSource === "audio") {

	source.connect(
		track1Gain
	);

}

track1Gain.connect(
	gainNode
);
	gainNode.gain.cancelScheduledValues(
		audioContext.currentTime
	);
	gainNode.gain.setValueAtTime(
		gainNode.gain.value,
		audioContext.currentTime
	);
	startTime =
		audioContext.currentTime;
	source.start();
	const currentSource = source;
	source.onended = () => {
		if (source === currentSource) {
			source = null;
		}
		cancelAnimationFrame(animationId);
		if (spectrumAnimationId) {
			cancelAnimationFrame(
				spectrumAnimationId
			);
		}
		if (vuAnimationId) {
			cancelAnimationFrame(
				vuAnimationId
			);

			vuAnimationId = null;
		}

		vuStarted = false;
		drawWaveform();
		setStatus("Playback finished");
		setLimiterStatus("Ready");
	};
	setStatus("Audio started");
	startVUMeter();
	startSpectrum();
	animatePlayhead();
}

function startSpectrum() {

	if (
    !source &&
    !micSource &&
    !drumMachine.playing &&
    track1ConnectedSource !== "looper"
) return;
	if (!analyserNode) return;
	if (spectrumAnimationId) {
		cancelAnimationFrame(
			spectrumAnimationId
		);
		spectrumAnimationId = null;
	}
	const canvas =
		document.getElementById(
			"spectrum"
		);
	if (!canvas) return;
	if (spectrumAnimationId) {
		cancelAnimationFrame(
			spectrumAnimationId
		);
	}
	const ctx =
		canvas.getContext(
			"2d", {
				alpha: false
			}
		);

	canvas.width =
    canvas.clientWidth;
	canvas.height =
    canvas.clientHeight;
	analyserNode.fftSize = 1024;
	analyserNode.minDecibels = -100;
	analyserNode.maxDecibels = -30;
	analyserNode.smoothingTimeConstant = 0.45;
	const bufferLength =
		analyserNode
		.frequencyBinCount;
	const data =
		new Uint8Array(
			bufferLength
		);
	const peaks =
		new Array(bufferLength)
		.fill(0);

	function draw() {
		if (
    !source &&
    !micSource &&
    !drumMachine.playing &&
    track1ConnectedSource !== "looper"
) {

    spectrumAnimationId = null;
    return;

}
		activeAnalyser
	.getByteFrequencyData(
		data
	);
		ctx.clearRect(
			0,
			0,
			canvas.width,
			canvas.height
		);

		const barWidth =
			canvas.width /
			bufferLength;
		let x = 0;
		for (
			let i = 0; i < bufferLength; i++
		) {
			const value =
				data[i];

			const height =
				Math.pow(
					value / 255,
					0.6
				) *
				canvas.height * 1.15;
			if (height > peaks[i]) {
				peaks[i] = Math.min(
					height,
					canvas.height - 4
				);

			} else {
				peaks[i] -= 2;
				if (peaks[i] < 0)
					peaks[i] = 0;
			}
			let r = 0;
			let g = 255;
			let b = 160;
			if (value > 180) {
				r = 255;
				g = 120;
				b = 0;
			} else if (value > 120) {
				r = 255;
				g = 220;
				b = 0;
			}
			ctx.fillStyle =
				"rgb(" +
				r + "," +
				g + "," +
				b + ")";
			ctx.fillRect(
				x,
				canvas.height - height,
				barWidth - 1,
				height
			);

			ctx.fillStyle =
				"rgb(255,255,255)";
			ctx.fillRect(
				x,
				canvas.height -
				peaks[i] - 2,
				barWidth - 1,
				2
			);
			x += barWidth;
		}

		spectrumAnimationId =
			requestAnimationFrame(
				draw
			);
	}
	draw();
}

function stopAudio() {
	if (!source) return;
	try {
		source.stop();
	} catch (e) {}
	source = null;
	cancelAnimationFrame(animationId);
	if (spectrumAnimationId) {
		cancelAnimationFrame(
			spectrumAnimationId
		);
	}
	if (vuAnimationId) {
		cancelAnimationFrame(
			vuAnimationId
		);
		vuAnimationId = null;
	}
	vuStarted = false;
	drawWaveform();
	setStatus("Audio stopped");
	setLimiterStatus(
		"Ready"
	);
}

function animatePlayhead() {
	if (!buffer) return;
	const canvas =
		document.getElementById("waveform");
	if (!canvas) return;
	const ctx =
		canvas.getContext("2d");

	function draw() {
		if (!source) return;
		drawWaveform();
		const elapsed =
			audioContext.currentTime -
			startTime;
		const duration =
			buffer.duration;
		const progress =
			elapsed / duration;
		const x =
			progress *
			canvas.width;
		ctx.beginPath();
		ctx.moveTo(x, 0);
		ctx.lineTo(
			x,
			canvas.height
		);
		ctx.strokeStyle = "red";
		ctx.lineWidth = 2;
		ctx.stroke();
		animationId =
			requestAnimationFrame(draw);
	}
	draw();
}

async function fadeInAudio() {

	if (!buffer || !gainNode) {
		setStatus("No file loaded");
		return;
	}
	await initAudio();
	if (source) {
		try {
			source.stop();
		} catch (e) {}
		source = null;
	}

	source = audioContext.createBufferSource();
	source.buffer = buffer;
	source.connect(gainNode);
	const now = audioContext.currentTime;
	gainNode.gain.cancelScheduledValues(now);
	gainNode.gain.setValueAtTime(0, now);
	gainNode.gain.linearRampToValueAtTime(1, now + 5);
	startTime = audioContext.currentTime;
	source.start();
	animatePlayhead();
	setStatus("Fade in runs");
}

function fadeOutAudio() {

	if (!source || !gainNode) {
		setStatus("Audio isn't playing");
		return;
	}
	const now = audioContext.currentTime;
	gainNode.gain.cancelScheduledValues(now);
	gainNode.gain.setValueAtTime(gainNode.gain.value, now);
	gainNode.gain.linearRampToValueAtTime(0, now + 5);
	setTimeout(() => {
		if (source) {
			try {
				source.stop();
			} catch (e) {}

			source = null;
		}

	}, 5000);
	setStatus("Fade out runs");
}

function cleanAudio() {
	if (!buffer) return;
	for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
		const data = buffer.getChannelData(ch);
		let peak = 0;
		for (let i = 0; i < data.length; i++) {
			let s = data[i];

			if (Math.abs(s) < 0.02) {
				s = 0;
			}
			if (Math.abs(s) > 0.6) {
				s *= 0.9;
			}
			if (s > 0.98) s = 0.98;
			if (s < -0.98) s = -0.98;
			if (Math.abs(s) > peak) {
				peak = Math.abs(s);
			}

			data[i] = s;
		}
		if (peak > 0) {
			const target = 0.95;
			const gain =
				target / peak;
			for (let i = 0; i < data.length; i++) {
				data[i] *= gain;
			}
		}
	}

	refreshOriginalBuffer();
	playAudio();
}

function changeVolume(factor) {
	if (!gainNode) return;
	let current =
		gainNode.gain.value;
	let newValue =
		current * factor;
	newValue =
		Math.max(
			0,
			Math.min(
				6,
				newValue
			)
		);
	gainNode.gain.cancelScheduledValues(
		audioContext.currentTime
	);
	gainNode.gain.setValueAtTime(
		newValue,
		audioContext.currentTime
	);
	const slider =
		document.getElementById("volume");
	if (slider) {
		slider.value =
			newValue;
	}
	setStatus(
		"Volume: " +
		newValue.toFixed(1)
	);
}

function normalizeAudio(target = 0.9) {
	if (!buffer) return;
	let max = 0;
	for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
		const data = buffer.getChannelData(ch);
		for (let i = 0; i < data.length; i++) {
			max = Math.max(max, Math.abs(data[i]));
		}
	}
	if (max === 0) return;
	const factor = target / max;
	for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
		const data = buffer.getChannelData(ch);
		for (let i = 0; i < data.length; i++) {
			data[i] *= factor;
		}
	}
}

function normalizeBuffer(targetBuffer, target = 0.9) {

	if (!targetBuffer) return;
	let max = 0;
	for (
		let ch = 0;
		ch < targetBuffer.numberOfChannels;
		ch++
	) {
		const data =
			targetBuffer.getChannelData(ch);
		for (
			let i = 0;
			i < data.length;
			i++
		) {
			max = Math.max(
				max,
				Math.abs(data[i])
			);
		}
	}

	if (max === 0) return;
	const factor =
		target / max;
	for (
		let ch = 0;
		ch < targetBuffer.numberOfChannels;
		ch++
	) {

		const data =
			targetBuffer.getChannelData(ch);
		for (
			let i = 0;
			i < data.length;
			i++
		) {

			data[i] *= factor;
		}
	}
}

function refreshOriginalBuffer() {
	if (!buffer || !audioContext)
		return;
	originalBuffer =
		audioContext.createBuffer(
			buffer.numberOfChannels,
			buffer.length,
			buffer.sampleRate
		);
	for (
		let ch = 0; ch < buffer.numberOfChannels; ch++
	) {
		originalBuffer
			.getChannelData(ch)
			.set(
				buffer.getChannelData(ch)
			);
	}
}

function dbToLinear(db) {
	return Math.pow(10, db / 20);
}

function applyLimiterToBuffer(
	targetBuffer,
	threshold = 0.9,
	knee = 0.25,
	ceiling = 1
) {
	if (!targetBuffer) return;
	let limiting = false;
	let clipping = false;
	for (
		let ch = 0; ch < targetBuffer.numberOfChannels; ch++
	) {
		const data =
			targetBuffer.getChannelData(ch);
		for (
			let i = 0; i < data.length; i++
		) {
			let x = data[i];
			let abs =
				Math.abs(x);
			if (abs >= ceiling * 0.98)
				clipping = true;
			if (abs > threshold) {
				limiting = true;
				let excess =
					abs - threshold;
				excess =
					excess /
					(
						1 +
						(
							excess /
							knee
						)
					);

				x =
					Math.sign(x) *
					(
						threshold +
						excess
					);

				x = Math.max(
					-ceiling,
					Math.min(
						ceiling,
						x
					)
				);
			}

			data[i] = x;
		}
	}

	if (clipping) {

		setLimiterStatus(
			"Clip!",
			"clip"
		);

	} else if (limiting) {

		setLimiterStatus(
			"Limiting",
			"limiting"
		);

	} else {

		setLimiterStatus(
			"Ready"
		);
	}
}

function updateLimiter() {

	if (!buffer || !originalBuffer)
		return;
	if (
		!Number.isFinite(limiterThreshold)
	) {
		limiterThreshold = -4;
	}

	if (
		!Number.isFinite(limiterCeiling)
	) {
		limiterCeiling = -0.3;
	}
	const threshold =
		dbToLinear(
			limiterThreshold
		);

	const ceiling =
		dbToLinear(
			limiterCeiling
		);

	for (
		let ch = 0; ch < originalBuffer.numberOfChannels; ch++
	) {

		buffer
			.getChannelData(ch)
			.set(
				originalBuffer.getChannelData(ch)
			);
	}

	applyLimiterToBuffer(
		buffer,
		threshold,
		0.25,
		ceiling
	);
	if (limiterNode) {

		limiterNode.threshold.value =
			limiterThreshold;
	}
}

function masteringLight() {
	if (!buffer) {
		setStatus("No file loaded");
		return;
	}
	normalizeAudio(0.9);
	applyLimiterToBuffer(
		buffer,
		dbToLinear(limiterThreshold),
		0.25,
		dbToLinear(limiterCeiling)
	);
	setStatus("Mastering Light ready");
	setLimiterStatus(
		"Active",
		"active"
	);
	refreshOriginalBuffer();
	playAudio();
}

let peakLevel = 0;
let rmsLevel = 0;
let peakHoldLevel = 0;
let vuStarted = false;
let peakTimeout = null;
let vuAnimationId = null;
let leftPeakLevel = 0;
let rightPeakLevel = 0;
let leftRmsLevel = 0;
let rightRmsLevel = 0;
let stereoWidth = 0;

function startVUMeter() {
    if (!activeAnalyser) {
	activeAnalyser = analyserNode;
}
	if (
		!audioContext ||
		!gainNode
	) return;

	if (vuAnimationId) {
		cancelAnimationFrame(
			vuAnimationId
		);
		vuAnimationId = null;
	}

	vuStarted = false;
	const data =
		new Uint8Array(
			analyserNode.frequencyBinCount
		);

	const channelData =
	new Float32Array(
		activeAnalyser.fftSize
	);

    const leftData =
	new Float32Array(
		leftAnalyser.fftSize
	);

const rightData =
	new Float32Array(
		rightAnalyser.fftSize
	);

	function update() {
		activeAnalyser.getFloatTimeDomainData(
	channelData
);
		let sum = 0;
		let peak = 0;
		let leftSum = 0;
		let rightSum = 0;
		let leftPeak = 0;
		let rightPeak = 0;
		let correlationSum = 0;
		activeAnalyser.getFloatTimeDomainData(
    channelData
);

leftAnalyser.getFloatTimeDomainData(
	leftData
);

rightAnalyser.getFloatTimeDomainData(
	rightData
);
		for (let i = 0; i < channelData.length; i++) {
			const sample =
				channelData[i];
			const abs =
				Math.abs(sample);
			if (abs > peak) {
				peak = abs;
			}

			if (
				abs >= 0.98
			) {
				if (peakIndicator) {
					peakIndicator.classList.add("active");
					if (peakTimeout) {
						clearTimeout(peakTimeout);
					}
					peakTimeout = setTimeout(() => {
						peakIndicator.classList.remove("active");
					}, 200);
				}
			}
			sum += sample * sample;

const left = leftData[i];
const right = rightData[i];
const leftAbs =
	Math.abs(left);
const rightAbs =
	Math.abs(right);

			if (leftAbs > leftPeak) {
				leftPeak = leftAbs;
			}
			if (rightAbs > rightPeak) {
				rightPeak = rightAbs;
			}
			leftSum += left * left;
			rightSum += right * right;
			correlationSum +=
				left * right;
		}
		const normalization =
	Math.sqrt(
		leftSum * rightSum
	);

const correlation =
	normalization > 0
		? correlationSum / normalization
		: 0;

		let rms =
	Math.sqrt(
		sum / channelData.length
	);

		peakLevel = peak;
		rmsLevel = rms;
		if (peak > peakHoldLevel) {
	peakHoldLevel = peak;
} else {

	peakHoldLevel -= 0.003;
	if (peakHoldLevel < 0) {
		peakHoldLevel = 0;
	}
}
		// STORE GLOBALS

		leftPeakLevel = leftPeak;
		rightPeakLevel = rightPeak;

		leftRmsLevel =
    Math.sqrt(leftSum / leftData.length);
rightRmsLevel =
    Math.sqrt(rightSum / rightData.length);

		// Stereo Width

		stereoWidth =
    (1 - correlation) / 2;

		const peakDb =
			20 * Math.log10(
				Math.max(peakLevel, 0.0001)
			);

		if (peakDb > -0.5) {}

		const rmsDb =
			20 * Math.log10(
				Math.max(rmsLevel, 0.0001)
			);

		const leftDb =
    20 * Math.log10(
        Math.max(leftRmsLevel, 0.000001)
    );

const rightDb =
    20 * Math.log10(
        Math.max(rightRmsLevel, 0.000001)
    );

// K-Weighting Annäherung

let weightedSum = 0;

for (
    let i = 0;
    i < channelData.length;
    i++
) {
    const sample =
        channelData[i];

    weightedSum +=
        sample * sample;
}

const weightedRms =
    Math.sqrt(
        weightedSum /
        channelData.length
    );

// EBU R128 Näherung

const lufs =
    -0.691 +
    20 *
    Math.log10(
        Math.max(
            weightedRms,
            0.000001
        )
    );

		const peakEl =
			document.getElementById("peakDb");
		const rmsEl =
			document.getElementById("rmsDb");
		const stereoEl =
			document.getElementById("stereoDb");
		const corrEl =
			document.getElementById("correlation");
        const widthEl =
	        document.getElementById("stereoWidth");
		const lufsEl =
			document.getElementById("lufsMeter");

		if (peakEl) {

			peakEl.textContent =
				"Peak: " +
				peakDb.toFixed(1) +
				" dBFS";
		}

		if (rmsEl) {

			rmsEl.textContent =
				"RMS: " +
				rmsDb.toFixed(1) +
				" dBFS";
		}

		if (stereoEl) {

			stereoEl.innerHTML =
				"L: " +
				leftDb.toFixed(1) +
				" dB<br>" +
				"R: " +
				rightDb.toFixed(1) +
				" dB";
		}

		if (corrEl) {

			corrEl.textContent =
				"CORR: " +
				correlation.toFixed(2);
		}

        if (widthEl) {

	widthEl.textContent =
		"WIDTH: " +
		(stereoWidth * 100).toFixed(0) +
		"%";

}
		if (lufsEl) {

			lufsEl.textContent =
				"Approx LUFS: " +
				lufs.toFixed(1);
		}

		// DISPLAY LEVEL

		const level =
			Math.min(
				100,
				rms * 220
			);

		const peakHoldHeight =
			Math.min(
				100,
				peakHoldLevel * 220
			);

		const track1Vu =
			document.getElementById("track1Vu");
		const masterVu =
			document.getElementById("masterVu");
		const peakHold =
			document.getElementById("peakHold");

		if (track1Vu) {
			track1Vu.style.height =
				level + "%";
		}

		// track2Vu wird durch
       // updateTrack2VU()
      // aktualisiert

		if (masterVu) {

			masterVu.style.height =
				level + "%";

		}

		// =====================
		// LARGE VU METER
		// =====================

		const masterVuLarge =
			document.getElementById(
				"masterVuLarge"
			);

		if (masterVuLarge) {

			masterVuLarge.style.height =
				level + "%";
		}

		// =====================
		// DB TEXT
		// =====================

		const vuDbText =
			document.getElementById(
				"vuDbText"
			);

		if (vuDbText) {

			vuDbText.innerText =
				peakDb.toFixed(1) +
				" dBFS";
		}

		if (peakHold) {

			peakHold.style.bottom =
				peakHoldHeight + "%";
		}

		if (
    !source &&
    !drumMachine.playing &&
    track1ConnectedSource !== "looper" &&
    activeAnalyser !== micAnalyser
) {

    vuAnimationId = null;
    vuStarted = false;
    return;

}

		vuAnimationId =
			requestAnimationFrame(update);
	}

	vuStarted = true;
	update();
}

// =====================
// MIC GATE
// =====================

function startMicGate() {

	if (micGateRunning) {
		return;
	}

	micGateRunning = true;

	function updateGate() {

		if (!micAnalyser || !micGateGain) {
			requestAnimationFrame(updateGate);
			return;
		}

		micAnalyser.getByteTimeDomainData(
			micGateData
		);

		let sum = 0;

		for (
			let i = 0;
			i < micGateData.length;
			i++
		) {

			const v =
				(micGateData[i] - 128) / 128;

			sum += v * v;
		}

		const rms =
			Math.sqrt(
				sum / micGateData.length
			);

		const openThreshold =
	micGateThreshold;

const closeThreshold =
	micGateThreshold * 0.7;

if (
	rms > openThreshold
) {

	micGateOpen = true;

}

if (
	rms < closeThreshold
) {

	micGateOpen = false;

}

const target =
	micGateOpen
		? 1.0
		: 0.0;

        const status =
	document.getElementById(
		"micStatus"
	);

if (status) {

	micGateState =
	target > 0;

updateMicStatus();

}

		micGateGain.gain.setTargetAtTime(
			target,
			audioContext.currentTime,
			0.03
		);

		requestAnimationFrame(
			updateGate
		);
	}

	updateGate();
}

function updateMicStatus() {

	const status =
		document.getElementById(
			"micStatus"
		);

	if (!status) {
		return;
	}

	if (
		!micInputGain ||
		micInputGain.gain.value === 0
	) {

		status.textContent =
			"MIC OFF";

		return;
	}

	status.textContent =
		(micGateState
			? "GATE OPEN"
			: "GATE CLOSED")
		+
		" | COMP "
		+
		micCompAmount.toFixed(1)
		+
		":1";
}

// =====================
// WAVEFORM
// =====================

function drawWaveform() {

	const canvas =
		document.getElementById("waveform");
	if (!canvas) return;
	canvas.width =
		canvas.clientWidth;
	canvas.height =
		canvas.clientHeight;
	const ctx =
		canvas.getContext("2d");
	ctx.clearRect(
		0,
		0,
		canvas.width,
		canvas.height
	);

	ctx.fillStyle = "#000";
	ctx.fillRect(
		0,
		0,
		canvas.width,
		canvas.height
	);

let data = null;
let liveSource = false;

if (
    track1ConnectedSource ===
    "audio"
) {

    liveSource = true;

}

else if (
    track1ConnectedSource ===
    "looper"
) {

    liveSource = true;

}

else if (
	track1ConnectedSource ===
	"drum"
	||
	track1ConnectedSource ===
	"midi"
) {

	liveSource = true;

}
	if (liveSource) {

    if (!activeAnalyser) {
        return;
    }

    const liveData =
        new Uint8Array(
            activeAnalyser.fftSize
        );

    activeAnalyser.getByteTimeDomainData(
        liveData
    );

    ctx.beginPath();

    for (
        let i = 0;
        i < canvas.width;
        i++
    ) {

        const index =
            Math.floor(
                i *
                liveData.length /
                canvas.width
            );

        const value =
            (liveData[index] - 128) / 128;

        const y =
            canvas.height / 2 +
            value *
            canvas.height * 0.45;

        if (i === 0) {

            ctx.moveTo(i, y);

        } else {

            ctx.lineTo(i, y);

        }

    }

    ctx.strokeStyle = "#22d3ee";
    ctx.lineWidth = 2;
    ctx.stroke();

    requestAnimationFrame(
        drawWaveform
    );

    return;
}

if (!data) {

    ctx.fillStyle = "#22d3ee";

    ctx.font = "14px sans-serif";

    ctx.textAlign = "center";

    ctx.fillText(
        track1ConnectedSource
            .toUpperCase(),
        canvas.width / 2,
        canvas.height / 2
    );

    return;

}

const step =
	Math.ceil(
		data.length /
		canvas.width
	);

	ctx.beginPath();

	for (
		let i = 0; i < canvas.width; i++
	) {
		let min = 1;
		let max = -1;
		for (
			let j = 0; j < step; j++
		) {

			const val =
				data[
					(i * step) + j
				];

			if (val < min)
				min = val;

			if (val > max)
				max = val;
		}

		const y1 =
			(1 + min) *
			canvas.height /
			2;

		const y2 =
			(1 + max) *
			canvas.height /
			2;

		ctx.moveTo(i, y1);
		ctx.lineTo(i, y2);
	}

	ctx.strokeStyle =
		"#22d3ee";
	ctx.lineWidth = 1;
	ctx.stroke();
}

// =====================
// WAVEFORM MIC
// =====================

 function drawMicWaveform() {

    if (!micAnalyser) return;

    const canvas =
        document.getElementById(
            "waveformMic"
        );

    if (!canvas) return;

    canvas.width =
        canvas.clientWidth;

    canvas.height =
        canvas.clientHeight;

    const ctx =
        canvas.getContext("2d");

    ctx.clearRect(
        0,
        0,
        canvas.width,
        canvas.height
    );

    ctx.fillStyle =
        "#020617";

    ctx.fillRect(
        0,
        0,
        canvas.width,
        canvas.height
    );

    const data =
        new Uint8Array(
            micAnalyser.fftSize
        );

    micAnalyser.getByteTimeDomainData(
        data
    );

    ctx.beginPath();

    const slice =
        canvas.width /
        data.length;

    let x = 0;

    for (
        let i = 0;
        i < data.length;
        i++
    ) {

        const y =
            (data[i] / 255) *
            canvas.height;

        if (i === 0) {

            ctx.moveTo(
                x,
                y
            );

        } else {

            ctx.lineTo(
                x,
                y
            );

        }

        x += slice;

    }

    ctx.strokeStyle =
        "#22d3ee";

    ctx.lineWidth = 2;

    ctx.stroke();

}

        function startMicWaveform() {

    function update() {

        drawMicWaveform();

        requestAnimationFrame(update);

    }

    update();

}

async function renderExportBuffer(
	sourceBuffer
) {
	const offline =
		new OfflineAudioContext(
			sourceBuffer.numberOfChannels,
			sourceBuffer.length,
			sourceBuffer.sampleRate
		);

	const source =
		offline.createBufferSource();

	source.buffer =
		sourceBuffer;

	// EQ

	const low =
		offline.createBiquadFilter();

	low.type = "lowshelf";
	low.frequency.value = 120;
	low.gain.value =
		eqLow?.gain.value || 0;

	const lowMid =
		offline.createBiquadFilter();

	lowMid.type = "peaking";
	lowMid.frequency.value = 400;
	lowMid.Q.value = 1;
	lowMid.gain.value =
		eqLowMid?.gain.value || 0;

	const mid =
		offline.createBiquadFilter();

	mid.type = "peaking";
	mid.frequency.value = 1200;
	mid.Q.value = 1;
	mid.gain.value =
		eqMid?.gain.value || 0;

	const highMid =
		offline.createBiquadFilter();

	highMid.type = "peaking";
	highMid.frequency.value = 3500;
	highMid.Q.value = 1;
	highMid.gain.value =
		eqHighMid?.gain.value || 0;

	const high =
		offline.createBiquadFilter();

	high.type = "highshelf";
	high.frequency.value = 9000;
	high.gain.value =
		eqHigh?.gain.value || 0;

	const limiter =
		offline.createDynamicsCompressor();

	limiter.threshold.value =
		limiterThreshold;

	limiter.knee.value = 0;
	limiter.ratio.value = 20;
	limiter.attack.value = 0.005;
	limiter.release.value = 0.15;

    const exportMasterLimiter =
    offline.createDynamicsCompressor();

exportMasterLimiter.threshold.value =
    -0.3;

exportMasterLimiter.knee.value = 0;
exportMasterLimiter.ratio.value = 20;
exportMasterLimiter.attack.value = 0.001;
exportMasterLimiter.release.value = 0.08;

    const exportFilter =
    offline.createBiquadFilter();

exportFilter.type =
    filterFX?.type || "lowpass";
exportFilter.frequency.value =
    filterFX?.frequency.value || 22000;
exportFilter.Q.value =
    filterFX?.Q.value || 1;

    const exportDistortionGain =
    offline.createGain();

exportDistortionGain.gain.value =
    distortionEnabled
        ? distortionGain?.gain.value || 0.45
        : 1;

const exportDistortionPreFilter =
    offline.createBiquadFilter();

exportDistortionPreFilter.type =
    "lowpass";
exportDistortionPreFilter.frequency.value =
    distortionPreFilter?.frequency.value || 8500;
exportDistortionPreFilter.Q.value =
    distortionPreFilter?.Q.value || 0.7;

const exportDistortion =
    offline.createWaveShaper();
const exportDrive =
    parseFloat(
        document.getElementById(
            "distortionDrive"
        )?.value || 0
    );

exportDistortion.curve =
    createDistortionCurve(
        Math.min(exportDrive, 12)
    );

exportDistortion.oversample =
    "4x";

const exportDistortionFilter =
    offline.createBiquadFilter();

exportDistortionFilter.type =
    "lowpass";

exportDistortionFilter.frequency.value =
    distortionFilter?.frequency.value || 6500;
exportDistortionFilter.Q.value =
    distortionFilter?.Q.value || 0.7;
const exportConvolver =
    offline.createConvolver();
exportConvolver.buffer =
    createReverbImpulse(
        parseFloat(
            document.getElementById(
                "reverbSize"
            )?.value || 1.5
        )
    );

const exportDryGain =
    offline.createGain();
exportDryGain.gain.value = 1;
const exportWetGain =
    offline.createGain();
exportWetGain.gain.value =
    reverbEnabled
        ? wetGain?.gain.value || 0.753
        : 0;

const exportReverbBoost =
    offline.createGain();
exportReverbBoost.gain.value =
    1.1;
const exportReverbMix =
    offline.createGain();
exportReverbMix.gain.value = 1;
        const exportDelay =
    offline.createDelay(2.0);
exportDelay.delayTime.value =
    delayNode?.delayTime.value || 0.35;
const exportDelayFeedback =
    offline.createGain();
exportDelayFeedback.gain.value =
    delayFeedback?.gain.value || 0.35;
const exportDelayWetGain =
    offline.createGain();
exportDelayWetGain.gain.value =
    delayEnabled
        ? delayWetGain?.gain.value || 0.35
        : 0;

const exportDelayMix =
    offline.createGain();
exportDelayMix.gain.value = 1;
        const exportSoftClipper =
    offline.createWaveShaper();

exportSoftClipper.curve =
    softClipperNode?.curve ||
    createSoftClipCurve(0.75);
exportSoftClipper.oversample =
    "4x";

source.connect(low);
low.connect(lowMid);
lowMid.connect(mid);
mid.connect(highMid);
highMid.connect(high);
high.connect(limiter);

limiter.connect(
    exportFilter
);

if (distortionEnabled) {

    exportFilter.connect(
        exportDistortionGain
    );

    exportDistortionGain.connect(
        exportDistortionPreFilter
    );

    exportDistortionPreFilter.connect(
        exportDistortion
    );

    exportDistortion.connect(
        exportDistortionFilter
    );

} else {

    exportFilter.connect(
        exportDistortionFilter
    );
}

// DRY

exportDistortionFilter.connect(
    exportDryGain
);

// WET

exportDistortionFilter.connect(
    exportConvolver
);

exportConvolver.connect(
    exportReverbBoost
);

exportReverbBoost.connect(
    exportWetGain
);

// MIX

exportDryGain.connect(
    exportReverbMix
);

exportWetGain.connect(
    exportReverbMix
);

// DELAY DRY

exportReverbMix.connect(
    exportDelayMix
);

// DELAY WET

exportReverbMix.connect(
    exportDelay
);

exportDelay.connect(
    exportDelayWetGain
);

exportDelayWetGain.connect(
    exportDelayMix
);

const outputGain =
    offline.createGain();

outputGain.gain.value =
    distortionEnabled
        ? 2.0
        : (
            fxOutputGain?.gain.value
            ?? 0.75
        );

if (softClipperEnabled) {

    exportDelayMix.connect(
        exportSoftClipper
    );

    exportSoftClipper.connect(
        outputGain
    );

} else {

    exportDelayMix.connect(
        outputGain
    );

}

outputGain.connect(
    exportMasterLimiter
);

exportMasterLimiter.connect(
    offline.destination
);

source.start(0);
return await offline.startRendering();
}

// =====================
// DOWNLOAD
// =====================

async function downloadAudio() {

	if (!buffer) {
		setStatus("No file loaded");
		return;
	}

	try {

		const bitDepth =
			parseInt(
				document
				.getElementById("bitDepth")
				?.value || 24
			);

		const sampleRate =
			parseInt(
				document
				.getElementById("sampleRate")
				?.value ||
				audioContext.sampleRate
			);

		const exportBuffer =
	await renderExportBuffer(
		buffer
	);

		if (
	document
	.getElementById(
		"normalizeExport"
	)?.checked
) {

	normalizeBuffer(
		exportBuffer,
		0.9
	);
}

		if (
			document
			.getElementById(
				"limiterExport"
			)?.checked
		) {

			applyLimiterToBuffer(
				exportBuffer,
				dbToLinear(limiterThreshold),
				0.25,
				dbToLinear(limiterCeiling)
			);
		}
                const renderedBuffer = exportBuffer;

		let finalBuffer;
if (
    renderedBuffer.sampleRate ===
    sampleRate
) {

    finalBuffer =
        renderedBuffer;

} else {

    finalBuffer =
        await resampleBuffer(
            renderedBuffer,
            sampleRate
        );
}

const wav =
	bufferToWave(
		finalBuffer,
		bitDepth
	);
		const blob =
			new Blob(
				[wav], {
					type: "audio/wav"
				}
			);

		const url =
			URL.createObjectURL(blob);
		const a =
			document.createElement("a");

		a.href = url;
		a.download =
			"jtfexport.wav";
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
		setStatus(
			"Download started"
		);

	} catch (err) {

		console.error(err);
		setStatus(
			"Download error"
		);
	}
}

// =====================
// START RECORDING
// =====================
function startRecording() {

	if (!recordingDestination) {

		setStatus("Recorder not ready");
		return;
	}

	recordedChunks = [];

	mediaRecorder =
		new MediaRecorder(
			recordingDestination.stream
		);

	mediaRecorder.ondataavailable =
		event => {

			if (
				event.data.size > 0
			) {

				recordedChunks.push(
					event.data
				);
			}
		};

	mediaRecorder.onstop =
		() => {

			document
				.getElementById(
					"recordStatus"
				).textContent =
				"Recording finished";
		};

	mediaRecorder.start();

	document
		.getElementById(
			"recordStatus"
		).textContent =
		"Recording...";

	setStatus(
		"Recording started"
	);
}

// =====================
// STOP RECORDING
// =====================
function stopRecording() {

	if (
		!mediaRecorder ||
		mediaRecorder.state ===
		"inactive"
	) return;

	mediaRecorder.stop();
	setStatus(
		"Recording stopped"
	);
}

// =====================
// DOWNLOAD RECORDING
// =====================

function downloadRecording() {

	if (
		recordedChunks.length === 0
	) {

		setStatus(
			"No recording"
		);

		return;
	}

	const blob =
		new Blob(
			recordedChunks, {
				type: "audio/webm"
			}
		);

	const url =
		URL.createObjectURL(
			blob
		);

	const a =
		document.createElement("a");

	a.href = url;
	a.download =
		"recording.webm";

	document.body.appendChild(
		a
	);

	a.click();
	document.body.removeChild(
		a
	);

	URL.revokeObjectURL(
		url
	);

	setStatus(
		"Recording downloaded"
	);
}

// =====================
// WAV CONVERTER
// =====================

async function resampleBuffer(
	sourceBuffer,
	targetSampleRate
) {
	if (
		sourceBuffer.sampleRate ===
		targetSampleRate
	) {
		return sourceBuffer;
	}

	const offlineContext =
		new OfflineAudioContext(
			sourceBuffer.numberOfChannels,
			Math.ceil(
				sourceBuffer.duration *
				targetSampleRate
			),
			targetSampleRate
		);

	const source =
		offlineContext.createBufferSource();
	source.buffer =
		sourceBuffer;
	source.connect(
		offlineContext.destination
	);
	source.start(0);
	return await offlineContext.startRendering();
}

function bufferToWave(
	abuffer,
	bitDepth = 24
) {

	const numOfChan =
		abuffer.numberOfChannels;

	const bytesPerSample =
		bitDepth === 16 ?
		2 :
		3;

	const length =
		abuffer.length *
		numOfChan *
		bytesPerSample +
		44;

	const arrayBuffer =
		new ArrayBuffer(length);

	const view =
		new DataView(arrayBuffer);

	let offset = 0;
	let pos = 0;

	function setUint16(data) {
		view.setUint16(
			pos,
			data,
			true
		);
		pos += 2;
	}

	function setUint32(data) {
		view.setUint32(
			pos,
			data,
			true
		);
		pos += 4;
	}

	setUint32(0x46464952);
	setUint32(length - 8);
	setUint32(0x45564157);
	setUint32(0x20746d66);
	setUint32(16);
	setUint16(1);
	setUint16(numOfChan);

	const sampleRate =
		abuffer.sampleRate;

	setUint32(
		sampleRate
	);

	setUint32(
		sampleRate *
		numOfChan *
		bytesPerSample
	);

	setUint16(
		numOfChan *
		bytesPerSample
	);
	setUint16(bitDepth);

	setUint32(0x61746164);
	setUint32(length - pos - 4);

	const channels = [];

	for (
		let i = 0; i < numOfChan; i++
	) {
		channels.push(
			abuffer.getChannelData(i)
		);
	}

	while (pos < length) {

		for (
			let i = 0; i < numOfChan; i++
		) {

			let sample =
				Math.max(
					-1,
					Math.min(
						1,
						channels[i][offset]
					)
				);

			// 24-Bit Bereich
			if (bitDepth === 16) {

				sample =
					sample < 0 ?
					sample * 32768 :
					sample * 32767;

				view.setInt16(
					pos,
					sample,
					true
				);

				pos += 2;

			} else {

				sample =
					sample < 0 ?
					sample * 8388608 :
					sample * 8388607;

				view.setUint8(
					pos,
					sample & 0xFF
				);

				view.setUint8(
					pos + 1,
					(sample >> 8) & 0xFF
				);

				view.setUint8(
					pos + 2,
					(sample >> 16) & 0xFF
				);

				pos += 3;

			}

		}

		offset++;
}

return arrayBuffer;

}

document
	.getElementById(
		"micToggle"
	)
	?.addEventListener(
		"click",
		toggleMicrophone
	);

const micGainSlider =
	document.getElementById(
		"micGain"
	);

if (micGainSlider) {

	micGainSlider.addEventListener(
		"input",
		e => {

			if (!micInputGain)
				return;

			micInputGain.gain.value =
				parseFloat(
					e.target.value
				);

		}
	);

}

const micCompSlider =
	document.getElementById(
		"micComp"
	);

if (micCompSlider) {

	micCompSlider.addEventListener(
		"input",
		e => {

			micCompAmount =
				parseFloat(
					e.target.value
				);

updateMicStatus();

			if (micCompressor) {

				micCompressor.ratio.value =
					micCompAmount;

			}

		}
	);

}

const micGateSlider =
	document.getElementById(
		"micGateThreshold"
	);

if (micGateSlider) {

	micGateSlider.addEventListener(
		"input",
		e => {

			micGateThreshold =
				parseFloat(
					e.target.value
				);

		}
	);

}

const micAmbienceSlider =
	document.getElementById(
		"micAmbience"
	);

if (micAmbienceSlider) {

	micAmbienceSlider.addEventListener(
		"input",
		e => {

			micAmbienceAmount =
				parseFloat(
					e.target.value
				);

			if (micAmbienceGain) {

				micAmbienceGain.gain.value =
					micAmbienceAmount;

			}

		}
	);

}

const track1Fader =
	document.getElementById(
		"track1Fader"
	);

if (track1Fader) {

	track1Fader.addEventListener(
		"input",
		e => {

			if (!track1Gain) {
				return;
			}

			track1Gain.gain.value =
				parseFloat(
					e.target.value
				) / 100;

		}
	);

}

const micFader =
	document.getElementById(
		"micFader"
	);

if (micFader) {

	micFader.addEventListener(
		"input",
		e => {

			if (!micGain) {
				return;
			}

			micGain.gain.value =
				parseFloat(
					e.target.value
				) / 100;

		}
	);

}

const micMute =
	document.getElementById(
		"micMute"
	);

if (micMute) {

	micMute.addEventListener(
		"click",
		() => {

			if (!micGain) {
				return;
			}

			micMuted =
				!micMuted;

			if (micMuted) {

				micGain.gain.value = 0;

				micMute.textContent =
					"Unmute";

			} else {

				const fader =
					document.getElementById(
						"micFader"
					);

				micGain.gain.value =
					parseFloat(
						fader.value
					) / 100;

				micMute.textContent =
					"Mute";

			}

		}
	);

}

const track1Mute =
	document.getElementById(
		"track1Mute"
	);

if (track1Mute) {

	track1Mute.addEventListener(
		"click",
		() => {

			if (!track1Gain) {
				return;
			}

			track1Muted =
				!track1Muted;

			if (track1Muted) {

				track1Gain.gain.value = 0;

				track1Mute.textContent =
					"Unmute";

			} else {

				const fader =
					document.getElementById(
						"track1Fader"
					);

				track1Gain.gain.value =
					parseFloat(
						fader.value
					) / 100;

				track1Mute.textContent =
					"Mute";

			}

		}
	);

}

const micSoloBtn =
	document.getElementById(
		"micSolo"
	);

if (micSoloBtn) {

	micSoloBtn.addEventListener(
		"click",
		() => {

			if (!track1Gain) {
				return;
			}

			micSolo =
				!micSolo;

			if (micSolo) {

				track1Gain.gain.value = 0;

				micSoloBtn.textContent =
					"Unsolo";

			} else {

				const track1Fader =
					document.getElementById(
						"track1Fader"
					);

				if (track1Fader) {

					track1Gain.gain.value =
						parseFloat(
							track1Fader.value
						) / 100;

				}

				micSoloBtn.textContent =
					"Solo";

			}

		}
	);

}

const track1SoloBtn =
	document.getElementById(
		"track1Solo"
	);

if (track1SoloBtn) {

	track1SoloBtn.addEventListener(
		"click",
		() => {

			if (!micGain) {
				return;
			}

			track1Solo =
				!track1Solo;

			if (track1Solo) {

				micGain.gain.value = 0;

				track1SoloBtn.textContent =
					"Unsolo";

			} else {

				const micFader =
					document.getElementById(
						"micFader"
					);

				if (micFader) {

					micGain.gain.value =
						parseFloat(
							micFader.value
						) / 100;

				}

				track1SoloBtn.textContent =
					"Solo";

			}

		}
	);

}

function updateMicVU() {

	if (!micAnalyser) {

		requestAnimationFrame(
			updateMicVU
		);

		return;
	}

	const data =
		new Uint8Array(
			micAnalyser.frequencyBinCount
		);

	micAnalyser.getByteTimeDomainData(
		data
	);

	let peak = 0;

	for (
		let i = 0;
		i < data.length;
		i++
	) {

		const sample =
			(data[i] - 128) / 128;

		peak = Math.max(
			peak,
			Math.abs(sample)
		);
	}

	const percent =
		Math.min(
			100,
			peak * 100
		);

	const vu =
		document.getElementById(
			"micVu"
		);

	if (vu) {

		vu.style.height =
			percent + "%";
	}

	requestAnimationFrame(
		updateMicVU
	);
}

function updateTrack2VU() {

	if (!micAnalyser) {
		requestAnimationFrame(
			updateTrack2VU
		);
		return;
	}

	const data =
		new Uint8Array(
			micAnalyser.frequencyBinCount
		);

	micAnalyser.getByteFrequencyData(
		data
	);

	let peak = 0;

	for (
		let i = 0;
		i < data.length;
		i++
	) {

		if (data[i] > peak) {
			peak = data[i];
		}

	}

	const vu =
		document.getElementById(
			"track2Vu"
		);

	if (vu) {

		vu.style.height =
			(peak / 255) * 100 + "%";

	}

	requestAnimationFrame(
		updateTrack2VU
	);

}

// =====================
// AUTO INIT
// =====================

document.addEventListener(
	"DOMContentLoaded",
	() => {

		initAudioTool();
		bindDrumSteps();
		setLimiterStatus(
			"Ready"
		);
	}
);

/* =========================
LOOPER & DRUM MACHINE
========================= */

const drumMachine = {

	kick: null,
	snare: null,
	hihat: null,
	tom: null,
	clap: null,
	hat: null,
	bell: null,
	crash: null,

	volume: 1,
	bpm: 120,
	swing: 0,
	humanize: 0,
	step: 0,
	timer: null,
	playing: false,
	pattern: {

kick: [
			0, 0, 0, 0,
			0, 0, 0, 0
		],

snare: [
			0, 0, 0, 0,
			0, 0, 0, 0
		],

hihat: [
    0, 0, 0, 0,
    0, 0, 0, 0
],

tom: [
    0, 0, 0, 0,
    0, 0, 0, 0
],

clap: [
    0, 0, 0, 0,
    0, 0, 0, 0
],

hat: [
    0, 0, 0, 0,
    0, 0, 0, 0
],

bell: [
    0, 0, 0, 0,
    0, 0, 0, 0
],

crash: [
    0, 0, 0, 0,
    0, 0, 0, 0
]
	}

};

const loops = [

	{
		buffer: null,
		source: null
	},

	{
		buffer: null,
		source: null
	},

	{
		buffer: null,
		source: null
	},

	{
		buffer: null,
		source: null
	}

];

let loopRecorder = null;

async function recordLoop(index) {

	await initAudio();

	const length =
		parseInt(
			document
			.getElementById("loopLength")
			.value
		);

	document
		.getElementById("looperStatus")
		.innerText =
		"Recording Loop " + (index + 1);

	let stream;

switch (
	loopSources[index]
) {

	case "mic":

    stream =
        micDestination.stream;

    break;

		case "drums":

		stream =
			drumDestination.stream;

		break;

	case "midi":

		stream =
			drumDestination.stream;

		break;

	case "master":

		stream =
			masterDestination.stream;

		break;

	default:

		stream =
			await navigator
				.mediaDevices
				.getUserMedia({
					audio: true
				});

}

	loopRecorder =
		new MediaRecorder(stream);

	const chunks = [];

	loopRecorder.ondataavailable =
		e => chunks.push(e.data);
	loopRecorder.onstop =
		async () => {

			const blob =
	new Blob(chunks);

if (
	blob.size === 0
) {

	document
		.getElementById(
			"looperStatus"
		).innerText =
		"No audio recorded";

	return;
}
			const arrayBuffer =
				await blob.arrayBuffer();
			const copy =
				arrayBuffer.slice(0);
			const audioBuffer =
				await audioContext
				.decodeAudioData(copy);

			loops[index].buffer =
				audioBuffer;

			document
				.getElementById("looperStatus")
				.innerText =
				"Loop " +
				(index + 1) +
				" ready";
		};

	loopRecorder.start();
	setTimeout(() => {
		loopRecorder.stop();
	}, length * 1000);
}

function playLoop(index) {

    if (!audioContext) return;

    if (audioContext.state !== "running") {
        audioContext.resume();
    }

    const loop = loops[index];

    if (loop.source) {
        loop.source.stop();
        loop.source = null;
    }

    if (!loop.buffer) return;

    const source = audioContext.createBufferSource();
    source.buffer = loop.buffer;
    source.loop = true;

    // GLOBAL STATE
    window.source = source;
    track1Source = "looper";
    track1ConnectedSource = "looper";

    // 🔥 EIN EINZIGER GAIN (kein doppelter Node!)
    const loopGain = audioContext.createGain();
    loopGain.gain.value = 1;

    // =========================
    // AUDIO ROUTING (WICHTIG!)
    // =========================

    source.connect(loopGain);

    loopGain.connect(track1Gain);
    track1Gain.connect(gainNode);

    // 🔥 ANALYSER MUSS HINTER MASTER GAIN HÄNGEN
    gainNode.connect(analyserNode);

    // optional: für VU/FFT korrekt setzen
    activeAnalyser = analyserNode;

    source.start();
startSpectrum();
startVUMeter();
drawWaveform();

    loop.source = source;

    document.getElementById("looperStatus").innerText =
        "Playing Loop " + (index + 1);
}
function stopLoop(index) {

	const loop =
		loops[index];

	if (loop.source) {

		loop.source.stop();
		loop.source =
			null;
	}
    window.source = null;
track1ConnectedSource = "audio";
activeAnalyser = analyserNode;
}

function clearLoop(index) {

	stopLoop(index);
	loops[index].buffer =
		null;

	document
		.getElementById("looperStatus")
		.innerText =
		"Loop " +
		(index + 1) +
		" cleared";
}

/* =========================
LOOPER EVENTS
========================= */

for (let i = 1; i <= 4; i++) {

	const rec =
		document.getElementById(
			"loop" + i + "Rec"
		);

	const play =
		document.getElementById(
			"loop" + i + "Play"
		);

	const stop =
		document.getElementById(
			"loop" + i + "Stop"
		);

	const clear =
		document.getElementById(
			"loop" + i + "Clear"
		);

	if (rec)
		rec.onclick =
		() => recordLoop(i - 1);

	if (play)
		play.onclick =
		() => playLoop(i - 1);

	if (stop)
		stop.onclick =
		() => stopLoop(i - 1);

	if (clear)
		clear.onclick =
		() => clearLoop(i - 1);
}

/* =========================
DRUM SAMPLE LOADER
========================= */

async function loadSampleFromUrl(
	type,
	url
) {

	const response =
		await fetch(url);

	const arrayBuffer =
		await response.arrayBuffer();

	const decoded =
		await audioContext.decodeAudioData(
			arrayBuffer
		);

	drumMachine[type] =
		decoded;
}

async function loadDrumSample(type, file) {

	if (!audioContext) {

		await initAudio();

	}

	const arrayBuffer =
		await file.arrayBuffer();
	const decoded =
		await audioContext.decodeAudioData(
			arrayBuffer.slice(0)
		);

	drumMachine[type] =
		decoded;
}

/* =========================
PLAY DRUM
========================= */

function playDrum(
	type,
	velocity = 1
) {

	if (!audioContext)
		return;

	const sample =
		drumMachine[type];

	if (!sample)
		return;

	const source =
		audioContext
		.createBufferSource();
	const gain =
		audioContext
		.createGain();

	gain.gain.value =
		drumMachine.volume *
		velocity;

	source.buffer =
	sample;

source.connect(gain);

gain.connect(track1Gain);
gain.connect(analyserNode);

track1Gain.connect(
    gainNode
);

if (drumDestination) {

	gain.connect(
		drumDestination
	);

}

source.start();
}

function runDrumStep() {

	const step =
		drumMachine.step;

	updateDrumStepIndicator(
		step
	);

	// KICK

	if (
		drumMachine.pattern.kick[step]
	) {

		let kickVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"kick",
			kickVelocity
		);
	}

	// SNARE

	if (
		drumMachine.pattern.snare[step]
	) {

		let snareVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"snare",
			snareVelocity
		);
	}

	// HIHAT

	if (
		drumMachine.pattern.hihat[step]
	) {

		let hihatVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"hihat",
			hihatVelocity
		);
	}

	// TOM

	if (
		drumMachine.pattern.tom[step]
	) {

		let tomVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"tom",
			tomVelocity
		);
	}

	// CLAP

	if (
		drumMachine.pattern.clap[step]
	) {

		let clapVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"clap",
			clapVelocity
		);
	}

	// HAT

	if (
		drumMachine.pattern.hat[step]
	) {

		let hatVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"hat",
			hatVelocity
		);
	}

	// BELL

	if (
		drumMachine.pattern.bell[step]
	) {

		let bellVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"bell",
			bellVelocity
		);
	}

	// CRASH

	if (
		drumMachine.pattern.crash[step]
	) {

		let crashVelocity =
			1 +
			(
				(
					Math.random() - 0.5
				) *
				drumMachine.humanize *
				0.4
			);

		playDrum(
			"crash",
			crashVelocity
		);
	}

	drumMachine.step++;

	if (
		drumMachine.step >= 8
	) {

		drumMachine.step = 0;
	}

    if (
    track1ConnectedSource ===
    "drum"
) {

    drawWaveform();

}

	if (
		drumMachine.playing
	) {

		clearTimeout(
			drumMachine.timer
		);

		const baseInterval =
			(60 / drumMachine.bpm) /
			2 *
			1000;

		let swingOffset = 0;

		if (
			drumMachine.step % 2
		) {

			swingOffset =
				baseInterval *
				drumMachine.swing;
		}

		const humanizeOffset =
			(
				Math.random() - 0.5
			) *
			drumMachine.humanize *
			30;

		const nextInterval =
			Math.max(
				20,
				baseInterval +
				swingOffset +
				humanizeOffset
			);

		drumMachine.timer =
			setTimeout(
				runDrumStep,
				nextInterval
			);
	}
}

/* =========================
STEP INDICATOR
========================= */

function updateDrumStepIndicator(
	activeStep
) {

	const buttons =
		document.querySelectorAll(
			".drum-step"
		);

	buttons.forEach(btn => {

		btn.classList.remove(
			"playing"
		);

		const step =
			parseInt(
				btn.dataset.step
			);

		if (
			step === activeStep
		) {

			btn.classList.add(
				"playing"
			);
		}
	});
}

/* =========================
START DRUM MACHINE
========================= */

function startDrumMachine() {

	if (
		drumMachine.playing
	) return;

	drumMachine.playing = true;
drumMachine.step = 0;

startVUMeter();
startSpectrum();

runDrumStep();

setStatus(
    "Drum Machine started"
);
}

/* =========================
STOP DRUM MACHINE
========================= */

function stopDrumMachine() {

	drumMachine.playing = false;

	if (
		drumMachine.timer
	) {

		clearTimeout(
			drumMachine.timer
		);

		drumMachine.timer =
			null;
	}

	setStatus(
		"Drum Machine stopped"
	);
}

/* =========================
TOGGLE STEP
========================= */

function toggleDrumStep(
	track,
	step
) {

	if (
		!drumMachine.pattern[track]
	) return;

	const value =
		drumMachine
		.pattern[track][step];

	drumMachine
		.pattern[track][step] =

		value ? 0 : 1;
}

/* =========================
DRUM STEP UI
========================= */

function bindDrumSteps() {

	if (
		!document.querySelector(
			".drum-step"
		)
	) return;

	document
		.querySelectorAll(
			".drum-step"
		)
		.forEach(btn => {

			btn.addEventListener(
				"click",
				() => {

					const track =
						btn.dataset.track;

					const step =
						parseInt(
							btn.dataset.step
						);

					toggleDrumStep(
						track,
						step
					);

					btn.classList.toggle(
						"active"
					);
				}
			);
		});
}

// =====================
// SOFT CLIP ROUTING
// =====================

function updateSoftClipRouting() {

	if (
	!masterGain ||
	!softClipperNode ||
	!masterLimiterNode ||
	!analyserNode ||
	!splitterNode
) return;

	try {
		masterGain.disconnect();
	} catch (e) {}

	try {
		softClipperNode.disconnect();
	} catch (e) {}

	try {
	masterLimiterNode.disconnect();
} catch (e) {}

if (softClipperEnabled) {

	masterGain.connect(
		softClipperNode
	);

	softClipperNode.connect(
		masterLimiterNode
	);

} else {

	masterGain.connect(
		masterLimiterNode
	);

	setStatus(
		"Soft Clipper disabled"
	);
}

masterLimiterNode.connect(
	analyserNode
);
    }

// =====================
// LIMITER UI EVENTS
// =====================

const limiterThresholdSlider =
	document.getElementById("limiterThreshold");
const limiterCeilingSlider =
	document.getElementById("limiterCeiling");
const softClipToggle =
	document.getElementById("softClipToggle");
const softClipAmount =
	document.getElementById("softClipAmount");

if (limiterThresholdSlider) {
	limiterThresholdSlider.addEventListener("input", (e) => {
		limiterThreshold = parseFloat(e.target.value);
		updateLimiter();
	});
}

if (limiterCeilingSlider) {
	limiterCeilingSlider.addEventListener("input", (e) => {
		limiterCeiling = parseFloat(e.target.value);
		updateLimiter();
	});
}

if (softClipToggle) {

	softClipToggle.addEventListener(
		"click",
		() => {

			softClipperEnabled = !softClipperEnabled;
			softClipToggle.textContent =
				softClipperEnabled ?
				"Soft Clip ON" :
				"Soft Clip OFF";

			updateSoftClipRouting();
		}
	);
}

if (softClipAmount) {

	softClipAmount.addEventListener(
		"input",
		(e) => {

			if (!softClipperNode)
				return;

			const amount =
				parseFloat(
					e.target.value
				);

			softClipperNode.curve =
				createSoftClipCurve(
					amount
				);
		}
	);
}

async function toggleMicrophone() {

	await initAudio();

	if (!micSource) {

		try {

			micStream =
				await navigator.mediaDevices.getUserMedia({
					audio: true
				});

			micSource =
				audioContext.createMediaStreamSource(
					micStream
				);

			micSource.connect(
	micInputGain
);

micInputGain.connect(
	micGateGain
);

micGateGain.connect(
	micCompressor
);

micCompressor.connect(
    micGain
);

micGain.connect(
    micMonitorGain
);

micMonitorGain.connect(
    masterGain
);

micGain.connect(
    micDestination
);

micCompressor.connect(
    micAmbienceGain
);

micAmbienceGain.connect(
	convolverNode
);

micInputGain.connect(
	micAnalyser
);
            startMicGate();

		} catch (err) {

			console.error(
				"Microphone error:",
				err
			);

			return;
		}

	}

	const button =
	document.getElementById(
		"micToggle"
	);

    const status =
	document.getElementById(
		"micStatus"
	);

if (micInputGain.gain.value > 0) {

	micInputGain.gain.value = 0;

	activeAnalyser =
		analyserNode;

	updateMicStatus();

	micGateRunning = false;

	if (button) {
		button.textContent =
	"MIC OFF";

if (status) {

}
	}

	} else {

	micInputGain.gain.value = 1;

	activeAnalyser =
    micAnalyser;

if (!vuStarted) {
    startVUMeter();
}

        startSpectrum();
        startMicWaveform();

updateMicStatus();

	if (button) {
		button.textContent =
	"MIC ON";

if (status) {


}
	}

}

}
