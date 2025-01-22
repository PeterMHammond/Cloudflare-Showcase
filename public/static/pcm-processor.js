class PCMProcessor extends AudioWorkletProcessor {
    process(inputs, outputs) {
        // Access the input audio data (only one channel in mono)
        const input = inputs[0];
        if (input && input[0]) {
            const channelData = input[0]; // Float32Array [-1.0, 1.0]

            // Convert float32 [-1, 1] to 8-bit PCM [0, 255]
            const pcmData = new Uint8Array(channelData.length);
            for (let i = 0; i < channelData.length; i++) {
                const sample = Math.max(-1, Math.min(1, channelData[i])); // Clamp to [-1, 1]
                pcmData[i] = Math.floor((sample + 1) * 127.5); // Scale to [0, 255]
            }

            // Send PCM data to the main thread
            this.port.postMessage(pcmData);
        }

        // Returning true keeps the processor alive
        return true;
    }
}

registerProcessor('pcm-processor', PCMProcessor); 