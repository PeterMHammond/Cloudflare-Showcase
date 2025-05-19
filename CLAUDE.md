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