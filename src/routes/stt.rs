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
    audio_data: Option<String>, // Base64 encoded WAV data
}

#[derive(Serialize, Deserialize, Debug)]
struct Word {
    word: String,
    start: f64,
    end: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Inputs {
    audio: Vec<u8>,
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

        // Send initial message
        let initial_message = TranscriptionResult {
            text: "Connected! Click Start Recording to begin...".to_string(),
            is_final: true,
            word_count: None,
            words: None,
            vtt: None,
            audio_data: None,
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
                    match self.process_audio_chunk(data).await {
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

    async fn process_audio_chunk(&mut self, audio_samples: Vec<u8>) -> Result<()> {
        let ai = self.env.ai("AI")?;
        
        // Create WAV header and combine with audio data
        let header = WavHeader::new(audio_samples.len() as u32);
        let mut wav_data = header.to_bytes();
        wav_data.extend_from_slice(&audio_samples);
        
        let input = Inputs {
            audio: wav_data.clone()
        };
        
        match ai.run("@cf/openai/whisper-tiny-en", &input).await {
            Ok(result) => {
                let result: serde_json::Value = result;
                
                // Extract all available fields from the response
                let text = result.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let word_count = result.get("word_count").and_then(|v| v.as_u64()).map(|v| v as u32);
                let words: Option<Vec<Word>> = result.get("words").and_then(|v| serde_json::from_value(v.clone()).ok());
                let vtt = result.get("vtt").and_then(|v| v.as_str()).map(|v| v.to_string());
                
                if !text.is_empty() {
                    // Convert WAV data to base64
                    let audio_base64 = STANDARD.encode(&wav_data);
                    
                    self.broadcast_transcription(TranscriptionResult {
                        text,
                        is_final: false,
                        word_count,
                        words,
                        vtt,
                        audio_data: Some(audio_base64),
                    }).await?;
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