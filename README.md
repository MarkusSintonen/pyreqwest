# pyreqwest - Powerful and fast Rust based HTTP client

#### Built on top of and inspired by [reqwest](https://github.com/seanmonstar/reqwest)

### Feature-rich
- High performance - 100% Rust codebase (zero-copy bodies, no `unsafe` code, no Python dependencies)
- Asynchronous and synchronous HTTP clients
- Customizable via middlewares and custom JSON serializers
- Ergonomic as `reqwest`
- HTTP/1.1 and HTTP/2 support (also HTTP/3 when it [stabilizes](https://docs.rs/reqwest/latest/reqwest/#unstable-features))
- Mocking and testing utilities (can also connect to ASGI apps)
- Fully type-safe with Python type hints
- Full test coverage
- Free threading

#### Standard HTTP features you would expect
- HTTPS support (using [rustls](https://github.com/rustls/rustls))
- Request and response body streaming
- Connection pooling
- JSON, URLs, Headers, Cookies etc. (all serializers in Rust)
- Automatic decompression (zstd, gzip, brotli, deflate)
- Automatic response decoding (charset detection)
- Multipart form support
- Proxy support
- Redirects
- Timeouts
- Authentication (Basic, Bearer)
- Cookie management

### Documentation

See [examples](https://github.com/MarkusSintonen/pyreqwest/tree/main/examples)
