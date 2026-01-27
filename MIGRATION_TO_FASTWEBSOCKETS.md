# Migration from tokio-tungstenite to fastwebsockets

## Summary

This document describes the migration of the sequencer_client library from `tokio-tungstenite` to `fastwebsockets` for WebSocket communication.

## Motivation

- **Performance**: fastwebsockets is designed for high-performance WebSocket communication with lower overhead
- **Modern Architecture**: Better integration with hyper 1.x and modern async Rust ecosystem
- **Lower Latency**: More direct control over frame handling without additional abstraction layers

## Changes Made

### Dependencies Updated

#### Cargo.toml Changes

**Removed:**
- `tokio-tungstenite = { version = "0", features = ["rustls-tls-native-roots"] }`
- `tokio-stream = "0.1.8"` (no longer needed)
- `http = "1.4.0"` (no longer needed)

**Added:**
- `fastwebsockets = { version = "0.8", features = ["upgrade"] }`

**Note:** fastwebsockets includes hyper as a transitive dependency, but we perform the WebSocket handshake manually without directly using hyper APIs.

### Code Changes

#### reader.rs

**Architecture Change:**

Previously, the library used `tokio_stream::StreamMap` to manage multiple WebSocket connections as streams. The new implementation uses a channel-based approach:

- Each WebSocket connection runs in its own dedicated tokio task
- Messages are sent through an `mpsc::unbounded_channel` to a central receiver
- This provides better separation of concerns and automatic reconnection handling per connection

**Key Implementation Details:**

1. **WebSocket Connection Function** (`connect_websocket_optimized`):
   - Now returns `WsStream` enum (either `Plain(FragmentCollector<TcpStream>)` or `Tls(FragmentCollector<TlsStream<TcpStream>>)`)
   - Performs manual HTTP/1.1 WebSocket upgrade handshake by writing raw HTTP request and reading response
   - No longer uses hyper for handshake (pure tokio AsyncRead/AsyncWrite)
   - Reads response byte-by-byte until `\r\n\r\n` to detect end of HTTP headers
   - Validates response contains "101" status and "Switching Protocols"
   - Handles both TLS (wss://) and plain (ws://) connections

2. **SequencerReader Structure**:
   - Changed from storing `StreamMap<u8, WebSocketStream<...>>` to storing `mpsc::UnboundedReceiver<(String, Instant)>`
   - Removed `url` field (URL is captured in spawned task closures)
   - Each connection spawns a long-lived task that handles reconnection automatically
   - Uses `WsStream` enum to handle both plain and TLS connections uniformly

3. **Connection Management**:
   - Each spawned task runs an infinite loop with automatic reconnection
   - Pattern matches on `WsStream` enum to handle both Plain and Tls connections
   - Handles frame types: `Text`, `Close`, `Ping` (with automatic Pong response)
   - Records receive timestamp immediately upon frame receipt for accurate latency tracking
   - Graceful error handling with 1-second delay on connection failures, 100ms delay before reconnect

4. **Message Processing**:
   - `into_stream()` method receives messages from channel instead of StreamMap
   - Timestamp is preserved from when the frame was received (not when processed)
   - All parsing logic remains unchanged

## Benefits

1. **Better Performance**: Direct frame handling without intermediate stream abstractions
2. **Lower Overhead**: Manual HTTP handshake eliminates hyper dependency overhead
3. **Improved Reconnection**: Each connection independently handles reconnection with proper backoff
4. **Automatic Ping/Pong**: Built-in handling of WebSocket keep-alive frames
5. **Cleaner Separation**: Each connection runs independently in its own task
6. **More Control**: Direct access to frame opcodes and payloads
7. **Simpler Dependencies**: Only fastwebsockets directly required (hyper is transitive)

## Compatibility

The public API remains unchanged:
- `SequencerReader::new()` signature is identical
- `into_stream()` returns the same stream type
- All message parsing logic is unchanged
- Users of the library do not need to change their code

## Testing

The migration was verified by:
1. Successful compilation with `cargo build --release`
2. No changes required to the main.rs example
3. Dependency tree confirmed removal of tokio-tungstenite
4. All parsing and message handling logic preserved

## Implementation Details

### Manual WebSocket Handshake

Instead of using hyper's high-level HTTP upgrade APIs, the implementation now performs a manual WebSocket handshake:

1. Constructs raw HTTP/1.1 upgrade request with WebSocket headers
2. Writes request bytes directly to TCP/TLS stream
3. Reads response byte-by-byte until `\r\n\r\n` (end of headers)
4. Validates response for "101 Switching Protocols"
5. Passes the stream directly to fastwebsockets

This approach:
- Avoids "upgrade expected but low level API in use" errors
- Provides full control over the handshake process
- Reduces dependency on hyper's connection management
- Works identically for both plain TCP and TLS connections

### Connection Architecture

- Multiple parallel WebSocket connections (configurable, default 10)
- Each connection runs in dedicated tokio task with infinite retry loop
- All connections send messages to single `mpsc::unbounded_channel`
- Central receiver processes messages in arrival order (first-arrival wins for latency)
- Deduplication by sequence number handles duplicate messages from multiple connections

## Notes

- TLS handling still uses `tokio-rustls` and `rustls-native-certs`
- TCP_NODELAY is still enabled for low-latency connections
- All original TCP buffer optimization comments are preserved
- No direct hyper dependency (only transitive through fastwebsockets)

## Migration Date

2024 (Specific date to be filled in)

## Author

Migration performed as part of library modernization effort.