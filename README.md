# Cloudflare Showcase

A showcase of Cloudflare Workers capabilities using Rust, demonstrating routing and API functionality.

## Setup

1. Install [wrangler](https://developers.cloudflare.com/workers/wrangler/install-and-update/):
```bash
npm install -g wrangler
```

2. Login to Cloudflare:
```bash
wrangler login
```

3. Set up Turnstile secrets:
```bash
# Set your Turnstile site key (from Cloudflare Turnstile configuration)
wrangler secret put TURNSTILE_SITE_KEY

# Set your Turnstile secret key (from Cloudflare Turnstile configuration)
wrangler secret put TURNSTILE_SECRET_KEY

# Set your cookie key (for session management)
wrangler secret put COOKIE_KEY
```

## Development

Run the development server:
```bash
wrangler dev --live-reload
```

## Deployment

Deploy to Cloudflare Workers:
```bash
wrangler deploy
```

