use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

#[wasm_bindgen]
pub struct WebsocketDO {
    state: State,
    _env: Env,
    is_new: bool,
}

impl WebsocketDO {
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

    async fn broadcast_time(&self) -> Result<()> {
        let web_socket_conns = self.state.get_websockets();
        if web_socket_conns.is_empty() {
            return Ok(());
        }

        let time_json = serde_json::json!({
            "id": "clock-display",
            "utc": Date::now().as_millis()
        });

        for conn in web_socket_conns {
            let _ = conn.send(&time_json);
        }
        Ok(())
    }

    async fn schedule_next_alarm(&self) -> Result<()> {
        self.state.storage().set_alarm(Duration::from_secs(1)).await
    }
}

#[durable_object]
impl DurableObject for WebsocketDO {
    fn new(state: State, env: Env) -> Self {
        Self { 
            state,
            _env: env,
            is_new: false
        }        
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        if !self.is_new {
            self.is_new = true;
            let _ = self.state.storage().delete_all().await?;
        }

        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        let pair = WebSocketPair::new()?;
        let server = pair.server;
        let client = pair.client;
        self.state.accept_web_socket(&server);

        let web_socket_conns = self.state.get_websockets();
        console_log!("web_socket_conns: {:?}", web_socket_conns.len());
        if web_socket_conns.len() == 1 {
            self.state.storage().set_alarm(Duration::from_secs(5)).await?;
        }
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

    async fn alarm(&mut self) -> Result<Response> {
        self.broadcast_time().await?;
        
        let web_socket_conns = self.state.get_websockets();
        if !web_socket_conns.is_empty() {
            self.schedule_next_alarm().await?;
        }
        
        Response::ok("")
    }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}
