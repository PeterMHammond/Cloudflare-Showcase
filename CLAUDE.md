# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Build Commands
```bash
# Install dependencies and build tools
npm install -g wrangler
cargo install worker-build

# Build the project
worker-build --release

# Development build
worker-build
```

### Development Server
```bash
# Run with live reload
wrangler dev --live-reload

# Run without live reload
wrangler dev
```

### Deployment
```bash
# Deploy to Cloudflare Workers
wrangler deploy
```

### Secret Management
```bash
# Set Turnstile site key
wrangler secret put TURNSTILE_SITE_KEY

# Set Turnstile secret key
wrangler secret put TURNSTILE_SECRET_KEY

# Set cookie key
wrangler secret put COOKIE_KEY
```

## Architecture Overview

This is a Cloudflare Workers application built with Rust, using the following key components:

### Core Structure
- **Worker Router**: Main entry point in `src/lib.rs` handles routing and middleware
- **Templates**: Askama HTML templates in `templates/` directory with base template inheritance
- **Components**: Reusable HTML components in `templates/components/`
- **Routes**: Individual route handlers in `src/routes/` directory

### Key Components
1. **Durable Objects**: Used for WebSocket connections, Study sessions, and Speech-to-Text functionality
   - `WebsocketDO`: Handles WebSocket connections
   - `StudyDO`: Manages study sessions
   - `SttDO`: Handles speech-to-text processing

2. **Middleware**: Validation state middleware in `src/utils/middleware.rs` for CSRF/Turnstile protection

3. **Template System**: Uses Askama for type-safe HTML templating with inheritance
   - Base template provides common layout and validation state
   - Each route has its own template extending the base

4. **Cloudflare Integrations**:
   - AI binding for text generation
   - Durable Objects for stateful operations
   - Turnstile for bot protection

### Route Patterns
Routes follow a consistent pattern:
1. Define template struct extending `BaseTemplate`
2. Create handler function taking `Request` and `RouteContext<ValidationState>`
3. Render template and return `Response`

### Configuration
- Build configuration in `wrangler.toml`
- Cargo dependencies in `Cargo.toml`
- Compatibility date: 2023-12-01

## Workflow Memories
- Update the version number in the Cargo.toml with each change, but do not worry about adding a note regarding this to the commit description.

## Datastar Implementation Guidelines

### Core Principles

1. **Hypermedia-Driven Architecture**
   - Use HTML as the primary UI medium (not JSON)
   - Minimize client-side JavaScript
   - Leverage server-side rendering with HTML fragments
   - Use Server-Sent Events (SSE) for real-time updates

2. **Declarative Reactivity**
   - Use HTML data attributes for state and behavior
   - Keep data binding in HTML templates
   - Let the server handle complex data transformations
   - Focus on minimal, performant UI updates

3. **Worker + Durable Object Structure**
   - Workers handle HTTP requests and responses
   - Durable Objects manage state and event streams
   - SSE communicates state changes to clients
   - Workers should do the heavy rendering work

### Implementation Pattern

```html
<!-- Component template with data bindings -->
<div data-signals='{{ initial_state_json }}'
     data-on-load="@get('/path/to/sse')">
    <input type="text" data-bind="message">
    <button data-on-click="@post('/path/to/endpoint')">Send</button>
    <div id="target-container">
        <!-- Content will be updated via SSE -->
    </div>
</div>
```

```rust
// In SSE event handler
let mut builder = SseBuilder::new();

// Add HTML fragments with selector targeting
builder.add_fragments_with_merge_options(
    rendered_html,
    MergeMode::Prepend,  // Merge mode determines how content is inserted
    "#target-container", // CSS selector for target element
    50,                  // Animation settle duration
    false                // Use view transition API
);

// Update client-side signals/state
builder.add_signals(&json!({
    "message": "New message",
    "count": 42
}))?;

// Return SSE response
builder.into_response(false) // Keep-alive flag
```

### Best Practices

1. **Minimize Client-Side State**
   - Keep most state on the server
   - Only pass necessary data to the client
   - Use SSE to keep client state updated

2. **Prefer Server Rendering**
   - Generate HTML fragments on the server
   - Keep DOM manipulation logic server-side
   - Only use client-side updates for immediate feedback

3. **SSE Connection Management**
   - Use a single SSE connection per client
   - Implement heartbeats to detect disconnections
   - Handle reconnection gracefully

4. **HTML-First Design**
   - Design components as HTML fragments
   - Use progressive enhancement
   - Keep semantic HTML as a priority

### Anti-Patterns to Avoid

1. **Don't implement complex client-side JS logic**
   - Avoid custom JavaScript frameworks
   - Don't duplicate server-side logic on the client
   - Let Datastar handle reactivity

2. **Don't misuse Durable Objects**
   - Don't create a DO per request
   - Don't store temporary or session data in DOs
   - Remember DOs have storage limits

3. **Don't over-fragment your HTML**
   - Keep related UI elements together
   - Avoid too fine-grained updates
   - Consider UI coherence in your fragment design

4. **Don't cache excessively**
   - Be careful with stale data
   - Design for real-time updates when needed
   - Balance performance and freshness