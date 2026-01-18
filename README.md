# SFU Gateway

A Rust HTTP server that acts as a gateway/load-balancer for multiple SFU instances.

## Overview

The gateway receives `/v1/channel` requests from Odoo and forwards them to an appropriate SFU based on:
- **Geographic region** - Route to the nearest SFU if region hint provided
- **Load distribution** - Round-robin across available instances

### JWT Key Management

```
Odoo ──JWT(gateway_key)──▶ Gateway ──JWT(sfu_key)──▶ SFU
```

- **Odoo** signs JWTs with the **gateway's key**
- **Gateway** verifies, selects an SFU, and re-signs with that **SFU's key**
- **Each SFU** only knows its own key

## Configuration

### Environment Variables

| Variable           | Default    | Description                            |
| ------------------ | ---------- | -------------------------------------- |
| `SFU_GATEWAY_BIND` | `0.0.0.0`  | Address to bind                        |
| `SFU_GATEWAY_PORT` | `8071`     | Port to listen on                      |
| `SFU_GATEWAY_KEY`  | (required) | JWT key for verifying tokens from Odoo |

### Secrets File

SFU entries are stored in a secrets file (default: `secrets.toml`):

```toml
[[sfu]]
address = "http://sfu1.example.com:3000"
region = "eu-west"
key = "sfu1-secret-key"

[[sfu]]
address = "http://sfu2.example.com:3000"
region = "us-east"
key = "sfu2-secret-key"
```

⚠️ **Security**: Protect this file with `chmod 600 secrets.toml`

## Quick Start

```bash
# Build
cargo build --release

# Configure
cp secrets.example.toml secrets.toml
chmod 600 secrets.toml
# Edit secrets.toml with your SFU entries

# Run
SFU_GATEWAY_KEY="your-gateway-key" cargo run -- --secrets secrets.toml
```

## API

### `GET /health`

Health check endpoint. Returns `{ "status": "ok" }`.

### `GET /v1/channel`

Create a channel on an SFU.

**Headers:** `Authorization: Bearer <JWT>` (signed with gateway's key)

**Query Parameters:**
- `region` (optional) - Preferred region for SFU selection
- `webRTC`, `recordingAddress` - Forwarded to SFU

**Response:** `{ "uuid": "...", "url": "http://sfu-address" }`

## Documentation

- [Integration Guide](doc/integration.md) - How to deploy between Odoo and SFUs
- [Load Balancing](doc/load_balancing.md) - Server selection strategy and region hints
- [Roadmap](doc/roadmap.md) - Future features and improvements
