use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use askama::Template;
use serde::{Deserialize, Serialize};
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

#[derive(Template)]
#[template(path = "stt.html")]
struct SttTemplate {
    base: BaseTemplate,
}

pub async fn handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "Speech to Text", "Speech to Text").await?;
    let template = SttTemplate { base };
    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(_err) => Response::error("Template Error", 500),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioChunk {
    chunk_type: String,  // "start" or "end"
    timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct TranscriptionResult {
    text: String,
    is_final: bool,
    word_count: Option<u32>,
    words: Option<Vec<Word>>,
    vtt: Option<String>,
    audio_data: Option<String>,
    language_probability: Option<f64>,
    avg_logprob: Option<f64>,
    no_speech_prob: Option<f64>,
    temperature: Option<f64>,
    compression_ratio: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Word {
    word: String,
    start: f64,
    end: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Inputs {
    audio: String,  // Base64 encoded audio data
}

#[derive(Serialize, Deserialize, Debug)]
struct TranscriptionInfo {
    language: Option<String>,
    language_probability: Option<f64>,
    duration: Option<f64>,
    duration_after_vad: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Segment {
    start: Option<f64>,
    end: Option<f64>,
    text: String,
    temperature: Option<f64>,
    avg_logprob: Option<f64>,
    compression_ratio: Option<f64>,
    no_speech_prob: Option<f64>,
    words: Option<Vec<Word>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WhisperResponse {
    transcription_info: Option<TranscriptionInfo>,
    text: String,
    word_count: Option<u64>,
    segments: Option<Vec<Segment>>,
    vtt: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AudioMessage {
    buffer: Vec<u8>,
    is_streaming: Option<bool>,
}

#[wasm_bindgen]
pub struct SttDO {
    state: State,
    env: Env,
    modified: Option<u64>,
}

#[durable_object]
impl DurableObject for SttDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            modified: None,
        }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        console_log!("Fetch request received for STT DO");
        self.update_modified().await?;

        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            console_log!("Not a WebSocket upgrade request");
            return Response::error("Expected Upgrade: websocket", 426);
        }

        let pair = WebSocketPair::new()?;
        let server = pair.server;
        let client = pair.client;

        self.state.accept_web_socket(&server);
        console_log!("New WebSocket connection accepted");

        // Send initial message as a proper TranscriptionResult
        let initial_message = TranscriptionResult {
            text: "Connected! Click Start Recording to begin...".to_string(),
            is_final: true,
            word_count: None,
            words: None,
            vtt: None,
            audio_data: None,
            language_probability: None,
            avg_logprob: None,
            no_speech_prob: None,
            temperature: None,
            compression_ratio: None,
        };
        match server.send(&initial_message) {
            Ok(_) => console_log!("Sent initial welcome message"),
            Err(e) => console_log!("Failed to send welcome message: {:?}", e),
        }

        Response::from_websocket(client)
    }

    async fn websocket_message(&mut self, _ws: WebSocket, message: WebSocketIncomingMessage) -> Result<()> {
        self.update_modified().await?;

        match message {
            WebSocketIncomingMessage::String(msg) => {
                console_log!("Received WebSocket message, length: {}", msg.len());
                match serde_json::from_str::<AudioChunk>(&msg) {
                    Ok(chunk) => {
                        console_log!("Processing control signal: {}", chunk.chunk_type);
                        match chunk.chunk_type.as_str() {
                            "start" => {
                                console_log!("Starting new audio stream");
                            },
                            "end" => {
                                console_log!("End of audio stream");
                            },
                            _ => console_log!("Unknown chunk type: {}", chunk.chunk_type),
                        }
                    }
                    Err(e) => console_log!("Failed to parse control message: {:?}", e),
                }
            }
            WebSocketIncomingMessage::Binary(data) => {
                console_log!("Received binary message of size: {}", data.len());
                
                if data.len() > 0 {
                    // Process raw audio data directly
                    match self.process_audio_chunk(data, true).await {
                        Ok(_) => console_log!("Successfully processed audio chunk"),
                        Err(e) => console_log!("Error processing audio chunk: {:?}", e),
                    }
                } else {
                    console_log!("Received empty audio chunk, skipping");
                }
            }
        }
        Ok(())
    }
}

impl SttDO {
    async fn update_modified(&mut self) -> Result<()> {
        self.modified = Some(Date::now().as_millis());
        self.state.storage().put("modified", self.modified).await?;
        Ok(())
    }

    // Confidence thresholds for transcription quality
    const MIN_AVG_LOGPROB: f64 = -0.7;          // Base threshold for avg_logprob
    const MIN_AVG_LOGPROB_SHORT: f64 = -0.9;    // More lenient threshold for short utterances
    const MAX_NO_SPEECH_PROB: f64 = 0.3;        // Increased to allow quieter speech
    const MAX_COMPRESSION_RATIO: f64 = 2.0;      // Keep this as is
    const MAX_TEMPERATURE: f64 = 0.5;           // Increased from 0.2 to allow more variation
    const MIN_WORD_DURATION: f64 = 0.03;        // Minimum word duration
    const MAX_WORD_DURATION: f64 = 2.0;         // Maximum reasonable word duration
    const MIN_CONFIDENCE_SCORE: f64 = 0.4;      // Threshold for overall confidence
    const MIN_AUDIO_ENERGY: f64 = 0.01;         // Threshold for audio energy
    const SHORT_UTTERANCE_WORDS: usize = 2;     // Number of words considered a short utterance

    fn calculate_confidence_score(segment: &Segment) -> f64 {
        // 1. First check critical thresholds that should fail immediately
        if let Some(prob) = segment.avg_logprob {
            // Use more lenient threshold for short utterances
            let is_short_utterance = segment.words.as_ref()
                .map(|words| words.len() <= Self::SHORT_UTTERANCE_WORDS)
                .unwrap_or(false);
            
            let threshold = if is_short_utterance {
                Self::MIN_AVG_LOGPROB_SHORT
            } else {
                Self::MIN_AVG_LOGPROB
            };

            if prob < threshold {
                console_log!("Failed confidence check: avg_logprob {:.3} below {} threshold {}", 
                    prob, 
                    if is_short_utterance { "short utterance" } else { "standard" },
                    threshold);
                return 0.0;
            }

            // Calculate normalized score relative to the threshold
            let norm_score = (prob - threshold) / (-0.1 - threshold);
            if norm_score <= 0.0 {
                console_log!("Failed confidence check: normalized avg_logprob score {:.3} too low", norm_score);
                return 0.0;
            }
        }

        if let Some(temp) = segment.temperature {
            if temp > Self::MAX_TEMPERATURE {
                console_log!("Failed confidence check: temperature {} above threshold {}", 
                    temp, Self::MAX_TEMPERATURE);
                return 0.0;
            }
        }

        if let Some(prob) = segment.no_speech_prob {
            if prob > Self::MAX_NO_SPEECH_PROB {
                console_log!("Failed confidence check: no_speech_prob {} above threshold {}", 
                    prob, Self::MAX_NO_SPEECH_PROB);
                return 0.0;
            }
        }

        // 2. Check word timings with strict validation
        if let Some(words) = &segment.words {
            for word in words {
                let duration = word.end - word.start;
                
                // Check for invalid timing
                if word.start == 0.0 && word.end == 0.0 {
                    console_log!("Failed confidence check: word '{}' has invalid timing", word.word);
                    return 0.0;
                }

                // Check for unreasonably long word duration
                if duration > Self::MAX_WORD_DURATION {
                    console_log!("Failed confidence check: word '{}' duration {}s exceeds maximum {}s", 
                        word.word, duration, Self::MAX_WORD_DURATION);
                    return 0.0;
                }

                // Check for unreasonably short duration based on word length
                let word_length = word.word.trim().len() as f64;
                let min_duration = Self::MIN_WORD_DURATION * word_length.max(1.0);
                if duration < min_duration {
                    console_log!("Failed confidence check: word '{}' duration {}s below minimum {}s for length {}", 
                        word.word, duration, min_duration, word_length);
                    return 0.0;
                }
            }

            // Check total segment duration vs word count
            let total_duration = words.last().map(|w| w.end).unwrap_or(0.0) 
                             - words.first().map(|w| w.start).unwrap_or(0.0);
            let word_count = words.len() as f64;
            let avg_duration = total_duration / word_count;
            if avg_duration > Self::MAX_WORD_DURATION {
                console_log!("Failed confidence check: average word duration {}s exceeds maximum", 
                    avg_duration);
                return 0.0;
            }
        }

        // 3. Calculate weighted score components
        let mut score = 1.0;

        // Weight avg_logprob heavily (80% of score)
        if let Some(prob) = segment.avg_logprob {
            let threshold = if segment.words.as_ref().map(|w| w.len() <= Self::SHORT_UTTERANCE_WORDS).unwrap_or(false) {
                Self::MIN_AVG_LOGPROB_SHORT
            } else {
                Self::MIN_AVG_LOGPROB
            };
            let norm_score = (prob - threshold) / (-0.1 - threshold);
            score *= 0.8 + (norm_score.max(0.0) * 0.2);
        }

        // Apply smaller penalties for other metrics
        if let Some(ratio) = segment.compression_ratio {
            if ratio > Self::MAX_COMPRESSION_RATIO {
                score *= 0.95; // Very small penalty
            }
        }

        if let Some(temp) = segment.temperature {
            score *= 1.0 - (temp / Self::MAX_TEMPERATURE) * 0.05; // Minimal temperature penalty
        }

        if let Some(prob) = segment.no_speech_prob {
            score *= 1.0 - (prob / Self::MAX_NO_SPEECH_PROB); // Scaled penalty based on threshold
        }

        console_log!("Final confidence score: {:.3}", score);
        score
    }

    async fn process_audio_chunk(&mut self, audio_samples: Vec<u8>, is_streaming: bool) -> Result<()> {
        // Calculate audio energy first
        let energy = Self::calculate_audio_energy(&audio_samples);
        if energy < Self::MIN_AUDIO_ENERGY {
            console_log!("Audio energy {} below threshold {}, skipping transcription", 
                energy, Self::MIN_AUDIO_ENERGY);
            return Ok(());
        }
        
        let ai = self.env.ai("AI")?;
        
        // Create WAV header and combine with audio data
        let header = WavHeader::new(audio_samples.len() as u32);
        let mut wav_data = header.to_bytes();
        wav_data.extend_from_slice(&audio_samples);
        
        // Convert to base64 once
        let audio_base64 = STANDARD.encode(&wav_data);
        
        match ai.run("@cf/openai/whisper-large-v3-turbo", &Inputs { audio: audio_base64.clone() }).await {
            Ok(result) => {
                let whisper_response: WhisperResponse = match serde_json::from_value(result) {
                    Ok(response) => response,
                    Err(e) => {
                        console_log!("Failed to parse Whisper response: {:?}", e);
                        return Ok(());
                    }
                };
                
                if !whisper_response.text.is_empty() {
                    console_log!("Processing non-empty transcription: \"{}\"", whisper_response.text);

                    // First check critical thresholds on first segment
                    let should_process = whisper_response.segments.as_ref()
                        .and_then(|segments| segments.first())
                        .map(|segment| {
                            // Use calculate_confidence_score to check all critical thresholds
                            let score = Self::calculate_confidence_score(segment);
                            if score == 0.0 {
                                console_log!("Failed critical thresholds check");
                                return false;
                            }
                            true
                        })
                        .unwrap_or(false);

                    if !should_process {
                        console_log!("Skipping transcription due to failed critical thresholds");
                        return Ok(());
                    }

                    // Only calculate overall confidence if critical thresholds pass
                    let overall_confidence = whisper_response.segments.as_ref()
                        .map(|segments| Self::calculate_overall_confidence(segments))
                        .unwrap_or(0.0);

                    console_log!("Overall confidence score: {:.3}", overall_confidence);
                    
                    if overall_confidence >= Self::MIN_CONFIDENCE_SCORE {
                        console_log!("VERIFIED HIGH CONFIDENCE: Broadcasting to {} clients", 
                            self.state.get_websockets().len());

                        // Get the first segment for additional metrics
                        let first_segment = whisper_response.segments.as_ref()
                            .and_then(|segments| segments.first());

                        self.broadcast_transcription(TranscriptionResult {
                            text: whisper_response.text,
                            is_final: !is_streaming,
                            word_count: whisper_response.word_count.map(|w| w as u32),
                            words: first_segment.and_then(|segment| segment.words.clone()),
                            vtt: whisper_response.vtt,
                            audio_data: Some(audio_base64),
                            language_probability: whisper_response.transcription_info
                                .and_then(|info| info.language_probability),
                            avg_logprob: first_segment.and_then(|s| s.avg_logprob),
                            no_speech_prob: first_segment.and_then(|s| s.no_speech_prob),
                            temperature: first_segment.and_then(|s| s.temperature),
                            compression_ratio: first_segment.and_then(|s| s.compression_ratio),
                        }).await?;
                        console_log!("Successfully sent transcription to client");
                    } else {
                        // Log detailed metrics for debugging
                        console_log!("Filtering out low confidence transcription:");
                        console_log!("  text: \"{}\"", whisper_response.text);
                        console_log!("  overall_confidence: {:.3} (threshold: {})", 
                            overall_confidence, Self::MIN_CONFIDENCE_SCORE);
                        console_log!("  audio_energy: {:.3} (threshold: {})", 
                            energy, Self::MIN_AUDIO_ENERGY);

                        if let Some(segment) = whisper_response.segments.as_ref().and_then(|s| s.first()) {
                            if let Some(prob) = segment.avg_logprob {
                                console_log!("  avg_logprob: {:.3} (threshold: {})", 
                                    prob, Self::MIN_AVG_LOGPROB);
                            }
                            if let Some(prob) = segment.no_speech_prob {
                                console_log!("  no_speech_prob: {:.3} (threshold: {})", 
                                    prob, Self::MAX_NO_SPEECH_PROB);
                            }
                            if let Some(ratio) = segment.compression_ratio {
                                console_log!("  compression_ratio: {:.3} (threshold: {})", 
                                    ratio, Self::MAX_COMPRESSION_RATIO);
                            }
                            if let Some(temp) = segment.temperature {
                                console_log!("  temperature: {:.3} (threshold: {})", 
                                    temp, Self::MAX_TEMPERATURE);
                            }
                        }
                    }
                }
            },
            Err(e) => {
                console_log!("Error from Whisper AI: {:?}", e);
            }
        }
        
        Ok(())
    }

    async fn broadcast_transcription(&self, result: TranscriptionResult) -> Result<()> {
        let web_socket_conns = self.state.get_websockets();
        for conn in web_socket_conns {
            let _ = conn.send(&result);
        }
        Ok(())
    }

    fn calculate_audio_energy(audio_samples: &[u8]) -> f64 {
        // Convert bytes to PCM samples (assuming 16-bit PCM)
        let samples: Vec<i16> = audio_samples.chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(i16::from_le_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect();

        if samples.is_empty() {
            return 0.0;
        }

        let sum_squares: f64 = samples.iter()
            .map(|&s| s as f64 * s as f64)
            .sum();
        (sum_squares / samples.len() as f64).sqrt()
    }

    fn calculate_overall_confidence(segments: &[Segment]) -> f64 {
        let mut total_score = 0.0;
        let mut count = 0;

        for segment in segments {
            let score = Self::calculate_confidence_score(segment);
            total_score += score;
            count += 1;
        }

        if count > 0 {
            total_score / count as f64
        } else {
            0.0
        }
    }
}

#[derive(Debug)]
struct WavHeader {
    chunk_id: [u8; 4],         // "RIFF"
    chunk_size: [u8; 4],       // File size - 8
    format: [u8; 4],           // "WAVE"
    subchunk1_id: [u8; 4],     // "fmt "
    subchunk1_size: [u8; 4],   // 16 for PCM
    audio_format: [u8; 2],     // PCM
    num_channels: [u8; 2],     // Mono
    sample_rate: [u8; 4],      // 16kHz
    byte_rate: [u8; 4],        // SampleRate * NumChannels * BitsPerSample/8
    block_align: [u8; 2],      // NumChannels * BitsPerSample/8
    bits_per_sample: [u8; 2], // 16 bits
    subchunk2_id: [u8; 4],     // "data"
    subchunk2_size: [u8; 4],   // data size
}

impl WavHeader {
    fn new(data_size: u32) -> Self {
        WavHeader {
            chunk_id: *b"RIFF",
            chunk_size: (data_size + 36).to_le_bytes(),
            format: *b"WAVE",
            subchunk1_id: *b"fmt ",
            subchunk1_size: 16u32.to_le_bytes(),
            audio_format: 1u16.to_le_bytes(),     // PCM
            num_channels: 1u16.to_le_bytes(),     // Mono
            sample_rate: 16000u32.to_le_bytes(),  // 16kHz
            byte_rate: 32000u32.to_le_bytes(),    // SampleRate * NumChannels * BitsPerSample/8
            block_align: 2u16.to_le_bytes(),      // NumChannels * BitsPerSample/8
            bits_per_sample: 16u16.to_le_bytes(), // 16 bits
            subchunk2_id: *b"data",
            subchunk2_size: data_size.to_le_bytes(),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(44);
        bytes.extend_from_slice(&self.chunk_id);
        bytes.extend_from_slice(&self.chunk_size);
        bytes.extend_from_slice(&self.format);
        bytes.extend_from_slice(&self.subchunk1_id);
        bytes.extend_from_slice(&self.subchunk1_size);
        bytes.extend_from_slice(&self.audio_format);
        bytes.extend_from_slice(&self.num_channels);
        bytes.extend_from_slice(&self.sample_rate);
        bytes.extend_from_slice(&self.byte_rate);
        bytes.extend_from_slice(&self.block_align);
        bytes.extend_from_slice(&self.bits_per_sample);
        bytes.extend_from_slice(&self.subchunk2_id);
        bytes.extend_from_slice(&self.subchunk2_size);
        bytes
    }
}

pub mod do_handler {
    use super::*;
    
    pub async fn handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
        let url = req.url()?;
        let path = url.path();

        // Both /stt/ws and /stt/audio should be handled by the DO
        if path == "/stt/ws" || path == "/stt/audio" {
            let namespace = ctx.env.durable_object("SttDO")?;
            let stub = namespace.id_from_name("SttDO")?.get_stub()?;
            stub.fetch_with_request(req).await
        } else {
            Response::error("Not found", 404)
        }
    }
} 