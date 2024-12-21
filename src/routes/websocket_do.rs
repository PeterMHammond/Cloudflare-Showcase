use worker::*;
use serde_json::json;
use wasm_bindgen::prelude::*;
use std::time::Duration;
use async_std::task;
use std::sync::{Arc, Mutex};

const WEBSOCKET_TEST_HTML: &str = r#"
<!DOCTYPE html>
<html lang="en" class="h-full">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Real-time Clock</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="h-full min-h-screen bg-[#e8e6de] p-4">
    <div class="max-w-4xl mx-auto space-y-4">
        <div class="bg-white rounded-lg shadow-sm p-8">
            <h1 class="text-2xl font-bold text-gray-800 mb-6">Real-time Clock</h1>
            <div class="flex flex-col items-center space-y-4">
                <div id="clock" class="text-4xl font-mono font-bold text-gray-800 bg-gray-50 px-8 py-4 rounded-lg shadow-inner">--:--:--</div>
                <div id="status" class="text-sm font-medium rounded-full px-4 py-1"></div>
            </div>
        </div>

        <div class="bg-white rounded-lg shadow-sm p-8">
            <h2 class="text-lg font-semibold text-gray-800 mb-4">Debug Log</h2>
            <div id="messages" class="font-mono text-sm bg-gray-50 rounded-lg p-4 h-[200px] overflow-y-auto space-y-1">
            </div>
        </div>
    </div>

    <script>
        const messagesDiv = document.getElementById('messages');
        const statusDiv = document.getElementById('status');
        const clockDiv = document.getElementById('clock');
        let ws = null;
        let clientId = localStorage.getItem('websocket_client_id');
        let reconnectTimer = null;
        let isReconnecting = false;
        
        function cleanupConnection() {
            if (ws) {
                try {
                    ws.close();
                } catch (e) {
                    console.log('Error closing WebSocket:', e);
                }
                ws = null;
            }
            if (reconnectTimer) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
        }

        function scheduleReconnect(delay = 1000) {
            if (!isReconnecting && !reconnectTimer) {
                isReconnecting = true;
                reconnectTimer = setTimeout(() => {
                    reconnectTimer = null;
                    isReconnecting = false;
                    if (!ws) connect();
                }, delay);
            }
        }
        
        function connect() {
            cleanupConnection();

            try {
                const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
                const url = `${protocol}//${window.location.host}/websocket_do${clientId ? '?id=' + clientId : ''}`;
                ws = new WebSocket(url);
                
                ws.onopen = () => {
                    statusDiv.textContent = 'Connected';
                    statusDiv.className = 'text-sm font-medium rounded-full px-4 py-1 bg-green-100 text-green-800';
                    console.log('WebSocket connected');
                    isReconnecting = false;
                };
                
                ws.onmessage = (event) => {
                    try {
                        const data = JSON.parse(event.data);
                        console.log('Received message:', data);
                        
                        if (data.type === 'client_id') {
                            clientId = data.id;
                            localStorage.setItem('websocket_client_id', clientId);
                            return;
                        }

                        if (data.type === 'ping') {
                            console.log('Ping received from server');
                            return;
                        }
                        
                        if (data.type === 'timestamp') {
                            const date = new Date(parseInt(data.timestamp));
                            const time = date.toLocaleTimeString();
                            clockDiv.textContent = time;
                            
                            const messageDiv = document.createElement('div');
                            messageDiv.className = 'text-gray-600';
                            messageDiv.textContent = `${time} - Timestamp received`;
                            messagesDiv.appendChild(messageDiv);
                            messagesDiv.scrollTop = messagesDiv.scrollHeight;
                        }
                    } catch (error) {
                        console.error('Error processing message:', error, event.data);
                    }
                };
                
                ws.onclose = (event) => {
                    console.log('WebSocket closed:', event.code, event.reason);
                    statusDiv.textContent = 'Disconnected - Reconnecting...';
                    statusDiv.className = 'text-sm font-medium rounded-full px-4 py-1 bg-red-100 text-red-800';
                    ws = null;

                    if (event.code !== 1000) {
                        scheduleReconnect();
                    }
                };

                ws.onerror = (error) => {
                    console.log('WebSocket error:', error);
                };
            } catch (error) {
                console.error('Error creating WebSocket:', error);
                scheduleReconnect();
            }
        }
        
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                cleanupConnection();
            } else {
                if (!ws && !isReconnecting) {
                    connect();
                }
            }
        });

        window.addEventListener('beforeunload', () => {
            cleanupConnection();
        });
        
        connect();
    </script>
</body>
</html>
"#;

#[derive(Clone)]
struct Client {
    id: String,
    socket: WebSocket,
}

struct SharedState {
    clients: Vec<Client>,
    clock_started: bool,
}

#[wasm_bindgen]
pub struct WebsocketDO {
    state: State,
    shared: Arc<Mutex<SharedState>>,
}

#[durable_object]
impl DurableObject for WebsocketDO {
    fn new(state: State, _env: Env) -> Self {
        Self { 
            state,
            shared: Arc::new(Mutex::new(SharedState {
                clients: Vec::new(),
                clock_started: false,
            })),
        }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        
        // Check if this is a WebSocket upgrade request
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        if url.path() == "/websocket_do" {
            match WebSocketPair::new() {
                Ok(pair) => {
                    let server = pair.server;
                    let client = pair.client;

                    // Accept the WebSocket connection
                    server.accept()?;

                    // Get or generate client ID
                    let client_id = url.query_pairs()
                        .find(|(key, _)| key == "id")
                        .map(|(_, value)| value.to_string())
                        .unwrap_or_else(|| format!("client-{}", Date::now().as_millis()));

                    // Send the client ID back to the client
                    let id_message = json!({
                        "type": "client_id",
                        "id": client_id
                    }).to_string();
                    server.send_with_str(&id_message)?;

                    // Add the client and check if we need to start the clock
                    let start_clock = {
                        let mut shared = self.shared.lock().unwrap();
                        
                        // Remove any existing connection with the same ID
                        shared.clients.retain(|c| {
                            if c.id == client_id {
                                return false;
                            }
                            // Test connection with ping message
                            match c.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                                Ok(_) => true,
                                Err(_) => {
                                    console_log!("Removing disconnected client {}: ping failed", c.id);
                                    false
                                }
                            }
                        });
                        
                        // Add the new client
                        shared.clients.push(Client {
                            id: client_id,
                            socket: server.clone(),
                        });

                        !shared.clock_started
                    };

                    // Start the global clock if needed
                    if start_clock {
                        let shared = Arc::clone(&self.shared);
                        
                        // Start the message sending loop
                        wasm_bindgen_futures::spawn_local(async move {
                            loop {
                                let timestamp = Date::now().as_millis();
                                let message = json!({
                                    "type": "timestamp",
                                    "timestamp": timestamp
                                }).to_string();
                                
                                let mut shared = shared.lock().unwrap();
                                shared.clients.retain(|client| {
                                    // First try a ping message
                                    if let Err(_) = client.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                                        console_log!("Removing disconnected client {}: ping failed", client.id);
                                        return false;
                                    }
                                    
                                    // Then try to send the message
                                    match client.socket.send_with_str(&message) {
                                        Ok(_) => true,
                                        Err(e) => {
                                            console_log!("Removing disconnected client {}: send failed - {:?}", client.id, e);
                                            false
                                        }
                                    }
                                });
                                
                                if shared.clients.is_empty() {
                                    shared.clock_started = false;
                                    break;
                                }
                                
                                drop(shared); // Release the lock before sleeping
                                task::sleep(Duration::from_secs(1)).await;
                            }
                        });
                        
                        // Mark the clock as started
                        self.shared.lock().unwrap().clock_started = true;
                    }

                    Response::from_websocket(client)
                }
                Err(e) => Response::error(e.to_string(), 500)
            }
        } else {
            Response::error("Not found", 404)
        }
    }
}

// WebSocket connection handler
pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}

// Test page handler
pub mod test {
    use super::*;

    pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
        let mut headers = Headers::new();
        headers.set("Content-Type", "text/html")?;

        let response_body = format!(
            "{}",
            WEBSOCKET_TEST_HTML
        );

        Ok(Response::ok(response_body)?.with_headers(headers))
    }
} 