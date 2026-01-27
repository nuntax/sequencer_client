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

**Added:**
- `fastwebsockets = { version = "0.8", features = ["upgrade"] }`
- `hyper = { version = "1", features = ["client", "http1"] }`
- `hyper-util = { version = "0.1", features = ["client", "client-legacy", "http1", "tokio"] }`
- `http-body-util = "0.1"`

### Code Changes

#### reader.rs

**Architecture Change:**

Previously, the library used `tokio_stream::StreamMap` to manage multiple WebSocket connections as streams. The new implementation uses a channel-based approach:

- Each WebSocket connection runs in its own dedicated tokio task
- Messages are sent through an `mpsc::unbounded_channel` to a central receiver
- This provides better separation of concerns and automatic reconnection handling per connection

**Key Implementation Details:**

1. **WebSocket Connection Function** (`connect_websocket_optimized`):
   - Now returns `FragmentCollector<TokioIo<Upgraded>>` instead of `WebSocketStream<MaybeTlsStream<TcpStream>>`
   - Uses hyper for HTTP/1.1 upgrade handshake
   - Manually constructs WebSocket upgrade request with proper headers
   - Handles both TLS (wss://) and plain (ws://) connections

2. **SequencerReader Structure**:
   - Changed from storing `StreamMap` to storing `mpsc::UnboundedReceiver`
   - Removed `url` field (URL is captured in spawn closures)
   - Each connection spawns a long-lived task that handles reconnection automatically

3. **Connection Management**:
   - Each spawned task runs an infinite loop with automatic reconnection
   - Handles frame types: `Text`, `Close`, `Ping` (with automatic Pong response)
   - Records receive timestamp immediately upon frame receipt for accurate latency tracking
   - Graceful error handling with exponential backoff on connection failures

4. **Message Processing**:
   - `into_stream()` method receives messages from channel instead of StreamMap
   - Timestamp is preserved from when the frame was received (not when processed)
   - All parsing logic remains unchanged

## Benefits

1. **Better Performance**: Direct frame handling without intermediate stream abstractions
2. **Improved Reconnection**: Each connection independently handles reconnection with proper backoff
3. **Automatic Ping/Pong**: Built-in handling of WebSocket keep-alive frames
4. **Cleaner Separation**: Each connection runs independently in its own task
5. **More Control**: Direct access to frame opcodes and payloads

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

## Notes

- The library now requires hyper 1.x ecosystem
- TLS handling still uses `tokio-rustls` and `rustls-native-certs`
- TCP_NODELAY is still enabled for low-latency connections
- All original TCP buffer optimization comments are preserved

## Migration Date

2024 (Specific date to be filled in)

## Author

Migration performed as part of library modernization effort.