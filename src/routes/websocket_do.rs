use worker::*;
use worker::console_log;
// use serde_json::json;
use wasm_bindgen::prelude::*;
use std::sync::{Arc, Mutex};
use async_std::stream::StreamExt;
use std::time::Duration;
use async_std::task;

#[derive(Clone)]
struct Client {
    id: String,
    socket: WebSocket,
}

struct SharedState {
    clients: Vec<Client>,
    clock_started: bool
}

impl SharedState {
    // Helper function to broadcast client count to all clients
    fn broadcast_client_count(&self, _time: u64) {
        let json_string = serde_json::to_string(&serde_json::json!({
            "id": "client-count",
            "value": self.clients.len()
        })).expect("Failed to serialize client_count");

        for client in &self.clients {
            let _ = client.socket.send_with_str(&json_string);
        }
    }
}

#[wasm_bindgen]
pub struct WebsocketDO {
    shared: Arc<Mutex<SharedState>>,
}

#[durable_object]
impl DurableObject for WebsocketDO {
    fn new(state: State, _env: Env) -> Self {
        Self { 
            shared: Arc::new(Mutex::new(SharedState {
                clients: Vec::new(),
                clock_started: false
            })),
        }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        let url = req.url()?;
        
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        match WebSocketPair::new() {
            Ok(pair) => {
                let server = pair.server;
                let client = pair.client;

                server.accept()?;

                let client_id = url.query_pairs()
                    .find(|(key, _)| key == "id")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_else(|| format!("client-{}", Date::now().as_millis()));

                let server_clone = server.clone();
                let client_id_clone = client_id.clone();
                let shared = Arc::clone(&self.shared);

                // Handle client messages
                wasm_bindgen_futures::spawn_local(async move {
                    let mut event_stream = server_clone.events().expect("Failed to get event stream");
                    
                    while let Some(event) = event_stream.next().await {
                        match event {
                            Ok(WebsocketEvent::Message(msg)) => {
                                if let Some(text) = msg.text() {
                                    console_log!("📥 {} Received: {}", Date::now().to_string(), text);
                                    // if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                                    //     let shared = shared.lock().unwrap();
                                        
                                    //     // Log the entire message for debugging
                                    //     // console_log!("Message data: {:?}", data);

                                    //     // Check if any checkbox state changed
                                    //     // let mut checkbox_changed = false;
                                    //     // let mut checkbox_name = String::new();
                                    //     // for (key, value) in data.as_object().unwrap_or(&serde_json::Map::new()) {
                                    //     //     if key.contains("checkbox") && key != "HEADERS" {
                                    //     //         checkbox_changed = true;
                                    //     //         checkbox_name = key.to_string();
                                    //     //         shared.shared_checkbox = value == "on";
                                    //     //         break;
                                    //     //     }
                                    //     // }

                                    //     // if checkbox_changed {
                                    //     //     let response = format!(r#"
                                    //     //         <div id="shared-checkbox-container" hx-swap-oob="true">
                                    //     //             <form ws-send hx-trigger="change from:input">
                                    //     //                 <label class="flex items-center space-x-2 mt-2">
                                    //     //                     <input type="checkbox" 
                                    //     //                            id="shared-checkbox-1"
                                    //     //                            name="{}"
                                    //     //                            value="on"
                                    //     //                            class="w-6 h-6 rounded"
                                    //     //                            {}>
                                    //     //                 </label>
                                    //     //             </form>
                                    //     //         </div>
                                    //     //         <div id="message-log" hx-swap-oob="beforeend">
                                    //     //             <div><span data-utc="{}"></span> Checkbox {}: {}</div>
                                    //     //         </div>
                                    //     //     "#, 
                                    //     //     checkbox_name,
                                    //     //     if shared.shared_checkbox { "checked" } else { "" },
                                    //     //     time,
                                    //     //     checkbox_name,
                                    //     //     shared.shared_checkbox
                                    //     //     );
                                            
                                    //     //     for client in &shared.clients {
                                    //     //         let _ = client.socket.send_with_str(&response);
                                    //     //     }
                                    //     // }
                                        
                                    //     // if let Some(form_data) = data.get("input-text") {
                                    //     //     if let Some(input_str) = form_data.as_str() {
                                    //     //         shared.shared_input = input_str.to_string();
                                                
                                    //     //         let response = format!(r#"
                                    //     //             <div id="shared-input" hx-swap-oob="true">
                                    //     //                 <input type="text" name="input-text" class="mt-2 px-4 py-2 border rounded-lg text-lg text-center" value="{}">
                                    //     //             </div>
                                    //     //             <div id="message-log" hx-swap-oob="beforeend">
                                    //     //                 <div><span data-utc="{}"></span> Input: {}</div>
                                    //     //             </div>
                                    //     //         "#,
                                    //     //         shared.shared_input,
                                    //     //         time,
                                    //     //         shared.shared_input
                                    //     //         );
                                                
                                    //     //         for client in &shared.clients {
                                    //     //             let _ = client.socket.send_with_str(&response);
                                    //     //         }
                                    //     //     }
                                    //     // }
                                    // }
                                }
                            },
                            Ok(WebsocketEvent::Close(_)) => {
                                let mut shared = shared.lock().unwrap();
                                shared.clients.retain(|c| c.id != client_id_clone);
                                
                                let time = Date::now().as_millis();
                                let disconnect_msg = format!(r#"
                                    <div id="message-log" hx-swap-oob="beforeend">
                                        <div class="text-yellow-500"><span data-utc="{}"></span> Client disconnected</div>
                                    </div>
                                "#, time);
                                
                                // Send disconnect message and update client count
                                for client in &shared.clients {
                                    let _ = client.socket.send_with_str(&disconnect_msg);
                                }
                                shared.broadcast_client_count(time);
                                break;
                            },
                            Err(_) => {
                                let mut shared = shared.lock().unwrap();
                                shared.clients.retain(|c| c.id != client_id_clone);
                                shared.broadcast_client_count(Date::now().as_millis());
                                break;
                            }
                        }
                    }
                });

                // Add new client and start clock if needed
                let start_clock = {
                    let mut shared = self.shared.lock().unwrap();
                    let now = Date::now().as_millis();

                    // Add the new client
                    shared.clients.push(Client {
                        id: client_id.clone(),
                        socket: server.clone(),
                    });

                    // Send initial state to the new client
                    // let initial_state = format!(r#"
                    //     <div id="clock-display" hx-swap-oob="true" data-utc="{}">{}</div>
                    //     <div id="client-count" hx-swap-oob="true">{}</div>
                    //     <div id="shared-checkbox-container" hx-swap-oob="true">
                    //         <form ws-send hx-trigger="change from:input">
                    //             <label class="flex items-center space-x-2 mt-2">
                    //                 <input type="checkbox" 
                    //                        id="shared-checkbox"
                    //                        name="checkbox-state"
                    //                        class="w-6 h-6 rounded"
                    //                        {}>
                    //             </label>
                    //         </form>
                    //     </div>
                    //     <div id="shared-input" hx-swap-oob="true">
                    //         <input type="text" 
                    //                name="input-text" 
                    //                class="mt-2 px-4 py-2 border rounded-lg text-lg text-center" 
                    //                value="{}">
                    //     </div>
                    //     <div id="message-log" hx-swap-oob="beforeend">
                    //         <div class="text-yellow-500"><span data-utc="{}"></span> Client connected</div>
                    //     </div>
                    // "#, 
                    // now,
                    // now,
                    // shared.clients.len(),
                    // if shared.shared_checkbox { "checked" } else { "" },
                    // shared.shared_input,
                    // now
                    // );
                    // let _ = server.send_with_str(&initial_state);

                    // Broadcast connect message and updated client count to all clients
                    let connect_msg = format!(r#"
                        <div id="message-log" hx-swap-oob="beforeend">
                            <div class="text-yellow-500"><span data-utc="{}"></span> Client connected</div>
                        </div>
                    "#, now);
                    
                    for client in &shared.clients {
                        if client.id != client_id {
                            let _ = client.socket.send_with_str(&connect_msg);
                        }
                    }
                    shared.broadcast_client_count(now);

                    !shared.clock_started
                };

                // Start the clock if needed
                if start_clock {
                    let shared = Arc::clone(&self.shared);
                    
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            task::sleep(Duration::from_secs(1)).await;
                            
                            let mut shared = shared.lock().unwrap();
                            if shared.clients.is_empty() {
                                shared.clock_started = false;
                                break;
                            }

                            let json_string = serde_json::to_string(&serde_json::json!({
                                "id": "clock-display",
                                "utc": Date::now().as_millis()
                            })).expect("Failed to serialize clock_data");
                            
                            for client in &shared.clients {
                                let _ = client.socket.send_with_str(&json_string);
                            }                            
                        }
                    });
                    
                    self.shared.lock().unwrap().clock_started = true;
                }

                Response::from_websocket(client)
            }
            Err(e) => Response::error(e.to_string(), 500)
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
} 