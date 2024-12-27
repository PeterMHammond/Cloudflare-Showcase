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
    async fn broadcast_client_count(&self) -> Result<()> {
        let count = self.state.storage().get::<i32>("connected_users").await.unwrap_or(0);
        
        let count_json = serde_json::json!({
            "id": "client-count",
            "value": count
        });

        let web_socket_conns = self.state.get_websockets();
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
        // let url = req.url()?;

        
        if !self.is_new {
            let delete_result = self.state.storage().delete_all().await?;
            console_log!("Delete result: {:?}", delete_result);
            self.state.storage().set_alarm(Duration::from_secs(5)).await?;
            self.is_new = true;
        }
        // Handle WebSocket upgrade
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        let pair = WebSocketPair::new()?;
        let server = pair.server;
        let client = pair.client;

        // Accept the WebSocket connection and store it in the Durable Object's state
        self.state.accept_web_socket(&server);

        // Update connected users count
        let current_connections = self.state.storage().get::<i32>("connected_users").await.unwrap_or(0);
        self.state.storage().put("connected_users", current_connections + 1).await?;


        let opts = ListOptions::new().prefix("control:");
        let keys_map = self.state.storage().list_with_options(opts).await.unwrap();
        
        for key_str in keys_map.keys().into_iter().map(|k| k.unwrap().as_string().unwrap()) {
            let _ = self.state.storage().get::<ControlMessage>(&key_str).await
                .and_then(|control_message| server.send(&control_message));
        }                

        self.broadcast_client_count().await?;

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
        let current_connections = self.state.storage().get::<i32>("connected_users").await.unwrap_or(0);
        if current_connections > 0 {
            self.state.storage().put("connected_users", current_connections - 1).await?;
        }
        self.broadcast_client_count().await?;
        Ok(())
    }

    async fn alarm(&mut self) -> Result<Response> {
        // Broadcast time to all connected clients
        self.broadcast_time().await?;
        
        // Schedule the next alarm if we still have clients
        let current_connections = self.state.storage().get::<i32>("connected_users").await.unwrap_or(0);
        if current_connections > 0 {
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
