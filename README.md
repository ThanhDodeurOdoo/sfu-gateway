# SFU Gateway

[![Tests](https://github.com/ThanhDodeurOdoo/sfu-gateway/actions/workflows/test.yml/badge.svg)](https://github.com/ThanhDodeurOdoo/sfu-gateway/actions/workflows/test.yml)

A Rust HTTP server that acts as a gateway/load-balancer for multiple [SFU](https://github.com/odoo/sfu) instances.

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

| Variable            | Default    | Description                            |
| ------------------- | ---------- | -------------------------------------- |
| `SFU_GATEWAY_BIND`  | `0.0.0.0`  | Address to bind                        |
| `SFU_GATEWAY_PORT`  | `8071`     | Port to listen on                      |
| `SFU_GATEWAY_KEY`   | (required) | JWT key for verifying tokens from Odoo |
| `SFU_GATEWAY_NODES` | (optional) | JSON string of SFU nodes (see below)   |


### JSON Configuration (Environment Variable)

For containerized environments where files are not preferred, you can pass the SFU list as a JSON string:

```bash
export SFU_GATEWAY_NODES='{
  "sfu": [
    {
      "address": "http://sfu1.example.com",
      "region": "eu-west",
      "key": "secret-key"
    }
  ]
}'
```

### Secrets File

As a less safe alternative, you can use a secret.toml file (default: `secrets.toml`):

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

The gateway prioritizes `SFU_GATEWAY_NODES` over the `secrets.toml` file.

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

- [Implementation Guide](doc/implementation.md) - How to deploy between Odoo and SFUs
- [Load Balancing](doc/load_balancing.md) - Server selection strategy and region hints
- [Roadmap](doc/roadmap.md) - Future features and improvements
