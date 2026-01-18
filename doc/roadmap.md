# Roadmap

## Phase 1: Health Checks

### Goal

Periodically poll SFUs to determine their availability and current load.

### Implementation

1. **New `/v1/load` Endpoint on SFU**  
   
   > [!NOTE]
   > The existing `/v1/stats` returns per-channel details (for debugging/monitoring).  
   > We add a new `/v1/load` route for lightweight aggregate metrics — no API version change needed.

   Response format:
   ```json
   {
     "channels": 12,
     "sessions": 45,
     "cpuUsage": 0.35,
     "memoryUsage": 0.48
   }
   ```
   
   This endpoint is unauthenticated (or uses a simple shared secret) for fast polling.

2. **Gateway Health Monitor**  
   The gateway spawns a background task that:
   - Polls each SFU's `/v1/stats` at a configurable interval (e.g., 10s)
   - Tracks response times and availability
   - Marks SFUs as unhealthy after consecutive failures

3. **Configuration**
   ```toml
   # Added to secrets.toml or passed via SFU_GATEWAY_NODES
   [health]
   interval_seconds = 10
   timeout_seconds = 5
   unhealthy_threshold = 3  # failures before marking unhealthy
   ```

### Outcome

The gateway maintains real-time knowledge of each SFU's status and load metrics.

---

## Phase 2: Load-Based Selection

### Goal

Replace round-robin with intelligent selection that routes to the least-loaded SFU.

### Selection Algorithm

```mermaid
flowchart TD
    A[Incoming Request] --> B{Region hint?}
    B -->|Yes| C[Filter by region]
    B -->|No| D[All SFUs]
    C --> E{Any healthy matches?}
    E -->|Yes| F[Healthy SFUs in region]
    E -->|No| D
    D --> G[Filter unhealthy SFUs]
    G --> H[Sort by load score]
    F --> H
    H --> I[Select lowest load]
    I --> J[Selected SFU]
```

### Load Score Calculation

Combine multiple factors into a weighted score:

```
load_score = (w1 × sessions/max_sessions) 
           + (w2 × cpu_usage) 
           + (w3 × memory_usage)
```

Default weights:
- `w1 = 0.5` (session count is primary factor)
- `w2 = 0.3` (CPU usage)  
- `w3 = 0.2` (memory usage)

### Configuration

```toml
[balancing]
strategy = "load"  # or "round-robin" for fallback

[balancing.weights]
sessions = 0.5
cpu = 0.3
memory = 0.2
```

---

## Phase 3: Graceful Degradation

### Goal

Handle partial failures and overload scenarios gracefully.

### Features

1. **Automatic Failover with Retry**  
   If the selected SFU fails to create a channel (timeout, 5xx, connection error), the gateway retries with the next-best candidate — up to `max_retries` attempts.

   ```mermaid
   flowchart LR
       A[Request] --> B[Select SFU]
       B --> C{Forward to SFU}
       C -->|Success| D[Return channel]
       C -->|Failure| E{Retries left?}
       E -->|Yes| F[Mark SFU down, select next]
       F --> C
       E -->|No| G[Return 503]
   ```

   **Configuration**:
   ```toml
   [failover]
   max_retries = 3  # try up to 3 different SFUs
   retry_timeout_ms = 5000
   ```

2. **Circuit Breaker**  
   Stop routing to SFUs that are consistently failing, with automatic recovery attempts.

3. **Overload Protection**  
   Reject requests if all SFUs exceed load thresholds or all retries exhausted (HTTP 503).

4. **Health Dashboard** (optional)  
   Expose `/v1/health` endpoint showing status of all SFUs for monitoring.

---

## Phase 4: Disconnect Support

### Problem

The current `/v1/disconnect` endpoint verifies requests by matching the caller's IP address against the channel's `remoteAddress`. With the gateway architecture, all requests originate from the gateway's IP.

### Solution

The gateway properly forwards the `X-Forwarded-For` header with the original Odoo IP.

The SFU's `extractRequestInfo` function (in `utils.ts`) reads the **first** IP from `X-Forwarded-For` when in proxy mode. So the gateway must **prepend** the original client IP:

```
X-Forwarded-For: <original-odoo-ip>, <gateway-ip>, ...
                  ↑ SFU reads this one
```

Implementation:
1. Read incoming `X-Forwarded-For` header (if present)
2. Prepend the original client IP
3. Forward the modified header to the SFU

Since the SFU already runs in proxy mode behind nginx, it already trusts and parses `X-Forwarded-For`.

> [!NOTE]
> No breaking changes required — the SFU's existing proxy mode handles this transparently.

---

## Future Considerations

- **Geographic Latency**: Factor in client-to-SFU latency measurements
- **Capacity Reservation**: Reserve headroom on each SFU for sudden spikes
- **Auto-scaling Integration**: Notify orchestrator when capacity is low
- **Secure and Scalable cross server**: make SFUs contact the gateway to register themselves.