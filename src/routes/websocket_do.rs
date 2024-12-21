use worker::*;
use serde_json::json;
use wasm_bindgen::prelude::*;
use std::time::Duration;
use async_std::task;
use std::sync::{Arc, Mutex};

const WEBSOCKET_TEST_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <title>WebSocket Test</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        #messages { 
            border: 1px solid #ccc; 
            padding: 20px; 
            height: 300px; 
            overflow-y: auto;
            margin-bottom: 20px;
        }
        .timestamp { color: #666; }
    </style>
</head>
<body>
    <h1>WebSocket Test</h1>
    <div id="messages"></div>
    <div id="status">Disconnected</div>

    <script>
        const messagesDiv = document.getElementById('messages');
        const statusDiv = document.getElementById('status');
        let ws = null;
        
        function connect() {
            // Close existing connection if any
            if (ws) {
                ws.close();
                ws = null;
            }

            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}/websocket_do`);
            
            ws.onopen = () => {
                statusDiv.textContent = 'Connected';
                statusDiv.style.color = 'green';
                console.log('WebSocket connected');
            };
            
            ws.onmessage = (event) => {
                console.log('Received message:', event.data);
                const data = JSON.parse(event.data);
                const time = new Date(parseInt(data.timestamp)).toLocaleTimeString();
                const messageDiv = document.createElement('div');
                messageDiv.innerHTML = `<span class="timestamp">${time}</span>`;
                messagesDiv.appendChild(messageDiv);
                messagesDiv.scrollTop = messagesDiv.scrollHeight;
            };
            
            ws.onclose = () => {
                statusDiv.textContent = 'Disconnected - Reconnecting...';
                statusDiv.style.color = 'red';
                console.log('WebSocket disconnected');
                ws = null;
                setTimeout(connect, 1000);
            };

            ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                ws = null;
            };
        }
        
        // Handle page visibility changes
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                // Page is hidden, close the connection
                if (ws) {
                    ws.close();
                    ws = null;
                }
            } else {
                // Page is visible again, reconnect
                connect();
            }
        });

        // Handle page unload
        window.addEventListener('beforeunload', () => {
            if (ws) {
                ws.close();
                ws = null;
            }
        });
        
        connect();
    </script>
</body>
</html>
"#;

struct SharedState {
    clients: Vec<WebSocket>,
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

                    // Add the client and check if we need to start the clock
                    let start_clock = {
                        let mut shared = self.shared.lock().unwrap();
                        // Clean up any closed connections
                        shared.clients.retain(|client| {
                            match client.send_with_str("ping") {
                                Ok(_) => true,
                                Err(_) => false
                            }
                        });
                        shared.clients.push(server.clone());
                        !shared.clock_started
                    };

                    // Start the global clock if needed
                    if start_clock {
                        let shared = Arc::clone(&self.shared);
                        
                        // Start the message sending loop
                        wasm_bindgen_futures::spawn_local(async move {
                            loop {
                                let timestamp = Date::now().as_millis();
                                let message = json!({ "timestamp": timestamp }).to_string();
                                
                                let mut shared = shared.lock().unwrap();
                                shared.clients.retain(|client| {
                                    match client.send_with_str(&message) {
                                        Ok(_) => true,
                                        Err(e) => {
                                            console_log!("Error sending message: {:?}", e);
                                            false // Remove failed client
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
        let url = req.url()?;
        let root_url = format!("{}://{}/", url.scheme(), url.host_str().unwrap_or("localhost"));

        let mut headers = Headers::new();
        headers.set("Content-Type", "text/html")?;

        let response_body = format!(
            "{}\n\nAvailable Routes:\n\
            - GET /websocket_do\n    WebSocket endpoint for real-time clock updates\n    Example: Connect via WebSocket to {root_url}websocket_do\n\
            - GET /websocket\n    This test page\n    Example: {root_url}websocket\n",
            WEBSOCKET_TEST_HTML
        );

        Ok(Response::ok(response_body)?.with_headers(headers))
    }
} 