use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::utils::scripture::get_scripture;
use crate::utils::middleware::ValidationState;

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum ControlValue {
    Bool(bool),
    Text(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct ControlMessage {
    control_name: String,
    control_value: ControlValue,
    control_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct TabBodyMessage {
    id: String,
    value: String,
}

#[wasm_bindgen]
pub struct StudyDO {
    state: State,
    env: Env,
}

impl StudyDO {
    async fn broadcast_client_count(&self, count: usize) -> Result<()> {        
        let web_socket_conns = self.state.get_websockets();
        if web_socket_conns.is_empty() {
            return Ok(());
        }

        let count_json = serde_json::json!({
            "id": "client-count",
            "value": count
        });

        for conn in web_socket_conns {
            let _ = conn.send(&count_json);
        }
        Ok(())
    }

    async fn process_scripture_control(&self, control_name: &str, text: &str) -> Result<()> {
        console_log!("Processing scripture control: {} with text: {}", control_name, text);
        
        // Check if this is the scriptures control
        if control_name == "scriptures" {
            console_log!("Found scriptures control, attempting to get scripture");
            
            let web_socket_conns = self.state.get_websockets();
            if web_socket_conns.is_empty() {
                return Ok(());
            }

            // Get all references, including empty lines
            let references: Vec<&str> = text.lines().collect();
            
            // Process each reference and send individual updates
            for (index, reference) in references.iter().enumerate() {
                let reference = reference.trim();
                
                let content = if !reference.is_empty() {
                    match get_scripture(reference, "LSB", &self.env).await {
                        Ok(scripture_text) => scripture_text,
                        Err(e) => {
                            console_log!("Error getting scripture for {}: {:?}", reference, e);
                            String::new()
                        }
                    }
                } else {
                    String::new()
                };

                let tab_body_msg = TabBodyMessage {
                    id: format!("scripture-content-{}", index),
                    value: content,
                };

                // console_log!("Sending scripture result for tab {}: {:?}", index, tab_body_msg);
                for conn in &web_socket_conns {
                    let _ = conn.send(&tab_body_msg);
                }
            }
        }
        Ok(())
    }
}

#[durable_object]
impl DurableObject for StudyDO {
    fn new(state: State, env: Env) -> Self {
        Self { 
            state,
            env,
        }        
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        let pair = WebSocketPair::new()?;
        let server = pair.server;
        let client = pair.client;
        self.state.accept_web_socket(&server);

        let web_socket_conns = self.state.get_websockets();
        console_log!("study_web_socket_conns: {:?}", web_socket_conns.len());
        self.broadcast_client_count(web_socket_conns.len()).await?;

        let opts = ListOptions::new().prefix("control:");
        let keys_map = self.state.storage().list_with_options(opts).await.unwrap();        
        for key_str in keys_map.keys().into_iter().map(|k| k.unwrap().as_string().unwrap()) {
            let _ = self.state.storage().get::<ControlMessage>(&key_str).await
                .and_then(|control_message| server.send(&control_message));
        }                

        Response::from_websocket(client)
    }

    async fn websocket_message(&mut self, ws: WebSocket, message: WebSocketIncomingMessage) -> Result<()> {
        match message {
            WebSocketIncomingMessage::String(msg) => {
                match serde_json::from_str::<ControlMessage>(&msg) {
                    Ok(control_message) => {
                        console_log!("Received message: {:?}", control_message);
                        let storage_key = format!("control:{}", control_message.control_name);
                        self.state.storage().put(&storage_key, &control_message).await?;
                        
                        if let ControlValue::Text(text) = &control_message.control_value {
                            self.process_scripture_control(&control_message.control_name, text).await?;
                        }
                        
                        let web_socket_conns = self.state.get_websockets();
                        web_socket_conns.iter()
                            .filter(|&conn| conn != &ws)
                            .for_each(|conn| {
                                let _ = conn.send(&control_message);
                            });
                    }
                    Err(e) => {
                        console_log!("Failed to parse control message: {:?}. Raw text: {}", e, msg);
                    }
                }
            }
            WebSocketIncomingMessage::Binary(_) => {
                console_log!("Binary messages are not supported");
            }
        }
        Ok(())
    }

    async fn websocket_close(&mut self, _ws: WebSocket, _code: usize, _reason: String, _was_clean: bool) -> Result<()> {
        let web_socket_conns = self.state.get_websockets();
        self.broadcast_client_count(web_socket_conns.len() - 1).await?;
        Ok(())
    }
}

pub async fn handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("StudyDO")?;
    let stub = namespace.id_from_name("StudyDO")?.get_stub()?;
    stub.fetch_with_request(req).await
} 