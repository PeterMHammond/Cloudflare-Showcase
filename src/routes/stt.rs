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
    // Base thresholds for regular speech
    const MIN_AVG_LOGPROB: f64 = -0.75;          // Base threshold for word predictions
    const MAX_NO_SPEECH_PROB: f64 = 0.1;         // Lower means more confident this is speech
    const MAX_COMPRESSION_RATIO: f64 = 2.0;       // Increased to allow for repetitive patterns
    const MAX_TEMPERATURE: f64 = 0.0;             // 0.0 means model was deterministic
    const MIN_DURATION: f64 = 0.15;              // Minimum duration for a word (in seconds)
    const MAX_DURATION: f64 = 0.75;              // Maximum duration for a word (in seconds)
    const MIN_CONFIDENCE_SCORE: f64 = 0.65;      // Slightly lowered composite confidence score
    
    // More lenient thresholds for numeric sequences
    const MIN_CONFIDENCE_SCORE_NUMBERS: f64 = 0.6; // More lenient composite score for numbers

    fn calculate_confidence_score(segment: &Segment) -> f64 {
        let mut score = 0.0;
        let mut weights = 0.0;

        // 1. Log probability score (weight: 0.35)
        let prob_score = if let Some(prob) = segment.avg_logprob {
            let score = (prob + 1.0).max(0.0);  // Normalize to 0.0-1.0
            console_log!("  1. Log probability: {} → score: {:.3} (weight: 0.35)", prob, score);
            score * 0.35
        } else {
            console_log!("  1. Log probability: missing");
            0.0
        };
        score += prob_score;
        weights += 0.35;

        // 2. Speech probability score (weight: 0.25)
        let speech_score = if let Some(prob) = segment.no_speech_prob {
            let score = 1.0 - prob;
            console_log!("  2. Speech probability: {} → score: {:.3} (weight: 0.25)", prob, score);
            score * 0.25
        } else {
            console_log!("  2. Speech probability: missing");
            0.0
        };
        score += speech_score;
        weights += 0.25;

        // 3. Compression ratio score (weight: 0.15)
        let compression_score = if let Some(ratio) = segment.compression_ratio {
            let score = (1.0 - (ratio - 0.5).max(0.0).min(0.5) * 2.0).max(0.0);
            console_log!("  3. Compression ratio: {} → score: {:.3} (weight: 0.15)", ratio, score);
            score * 0.15
        } else {
            console_log!("  3. Compression ratio: missing");
            0.0
        };
        score += compression_score;
        weights += 0.15;

        // 4. Temperature score (weight: 0.15)
        let temp_score = if let Some(temp) = segment.temperature {
            let score = (1.0 - temp).max(0.0);
            console_log!("  4. Temperature: {} → score: {:.3} (weight: 0.15)", temp, score);
            score * 0.15
        } else {
            console_log!("  4. Temperature: missing");
            0.0
        };
        score += temp_score;
        weights += 0.15;

        // 5. Word timing score (weight: 0.10)
        let timing_score = if let Some(words) = &segment.words {
            let mut word_scores = Vec::new();
            let mut total_score = 1.0;
            for word in words {
                let duration = word.end - word.start;
                let word_score = if duration < Self::MIN_DURATION {
                    duration / Self::MIN_DURATION
                } else if duration > Self::MAX_DURATION {
                    Self::MAX_DURATION / duration
                } else {
                    1.0
                };
                total_score *= word_score;
                word_scores.push((word.word.as_str(), duration, word_score));
            }
            console_log!("  5. Word timing scores:");
            for (word, duration, score) in word_scores {
                console_log!("     - '{}': {:.3}s → score: {:.3}", word, duration, score);
            }
            console_log!("     Final timing score: {:.3} (weight: 0.10)", total_score);
            total_score * 0.10
        } else {
            console_log!("  5. Word timing: missing");
            0.0
        };
        score += timing_score;
        weights += 0.10;

        // Calculate final normalized score
        let final_score = if weights > 0.0 { score / weights } else { 0.0 };
        console_log!("Final confidence score: {:.3} (normalized by {:.2} weights)", final_score, weights);
        
        final_score
    }

    fn is_high_confidence(segment: &Segment) -> bool {
        // First check if this is likely a number sequence
        let text = segment.text.trim();
        let is_number_sequence = text.chars().any(|c| c.is_numeric()) && 
                               text.chars().all(|c| c.is_numeric() || c.is_whitespace() || c == '.' || c == ',');
        
        // Calculate composite confidence score
        let confidence_score = Self::calculate_confidence_score(segment);
        
        // Use appropriate threshold based on content type
        let min_score = if is_number_sequence {
            Self::MIN_CONFIDENCE_SCORE_NUMBERS
        } else {
            Self::MIN_CONFIDENCE_SCORE
        };

        confidence_score >= min_score
    }

    async fn process_audio_chunk(&mut self, audio_samples: Vec<u8>, is_streaming: bool) -> Result<()> {
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
                
                console_log!("Whisper response: {:?}", whisper_response);
                
                if !whisper_response.text.is_empty() {
                    console_log!("Processing non-empty transcription: \"{}\"", whisper_response.text);
                    
                    let first_segment = whisper_response.segments.as_ref()
                        .and_then(|segments| segments.first());
                    
                    console_log!("Checking confidence metrics...");
                    // Check if this is a high confidence transcription using all metrics
                    let is_high_confidence = first_segment
                        .map(|segment| {
                            let result = Self::is_high_confidence(segment);
                            console_log!("Confidence check result: {}", result);
                            result
                        })
                        .unwrap_or_else(|| {
                            console_log!("No segment found, defaulting to low confidence");
                            false
                        });

                    if is_high_confidence {
                        console_log!("VERIFIED HIGH CONFIDENCE: Broadcasting to {} clients", 
                            self.state.get_websockets().len());
                        self.broadcast_transcription(TranscriptionResult {
                            text: whisper_response.text,
                            is_final: !is_streaming,  // Set is_final based on streaming status
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
                        // Log which metrics failed
                        if let Some(segment) = first_segment {
                            console_log!("Filtering out low confidence transcription:");
                            console_log!("  text: \"{}\"", whisper_response.text);
                            if let Some(prob) = segment.avg_logprob {
                                console_log!("  avg_logprob: {} (threshold: {}) - {}", 
                                    prob, Self::MIN_AVG_LOGPROB, 
                                    if prob > Self::MIN_AVG_LOGPROB { "PASS" } else { "FAIL" });
                            }
                            if let Some(prob) = segment.no_speech_prob {
                                console_log!("  no_speech_prob: {} (threshold: {}) - {}", 
                                    prob, Self::MAX_NO_SPEECH_PROB,
                                    if prob < Self::MAX_NO_SPEECH_PROB { "PASS" } else { "FAIL" });
                            }
                            if let Some(ratio) = segment.compression_ratio {
                                console_log!("  compression_ratio: {} (threshold: {}) - {}", 
                                    ratio, Self::MAX_COMPRESSION_RATIO,
                                    if ratio < Self::MAX_COMPRESSION_RATIO { "PASS" } else { "FAIL" });
                            }
                            if let Some(temp) = segment.temperature {
                                console_log!("  temperature: {} (threshold: {}) - {}", 
                                    temp, Self::MAX_TEMPERATURE,
                                    if temp <= Self::MAX_TEMPERATURE { "PASS" } else { "FAIL" });
                            }
                            console_log!("Transcription filtered out due to low confidence");
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