<p align="center">
    <img width="250" alt="logo2" src="https://github.com/user-attachments/assets/d93f91bd-5f2e-4fbc-80be-1b3344433853" />
</p>

<p align="center">
    <em>Python HTTP client fully in Rust</em>
</p>

---

pyreqwest - Powerful and fast Rust based HTTP client. Built on top of and inspired by [reqwest](https://github.com/seanmonstar/reqwest).

## Why

- No reinvention of the wheel - built on top of battle-tested reqwest and other Rust HTTP crates
- Secure and fast - no C-extension code, no Python code or dependencies, no `unsafe` code
- Ergonomic and easy to use - similar API as in reqwest, fully type-annotated
- Testing ergonomics - mocking included, ability to connect into ASGI apps

Using this is a good choice when:

- You care about throughput and latency
- You want a single solution to serve all your HTTP client needs

This is not a good choice when:

- You want a pure Python solution allowing debugging of the HTTP client internals
- You use alternative Python implementations or Python version older than 3.11

## Feature-rich

- High performance
- Asynchronous and synchronous HTTP clients
- Customizable via middlewares and custom JSON serializers
- Ergonomic as `reqwest`
- HTTP/1.1 and HTTP/2 support (also HTTP/3 when it [stabilizes](https://docs.rs/reqwest/latest/reqwest/#unstable-features))
- Mocking and testing utilities (can also connect to ASGI apps)
- Fully type-safe with Python type hints
- Full test coverage
- Free threading

### Standard HTTP features you would expect

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

## Quickstart

```python
#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["pyreqwest"]
# ///

from pyreqwest.client import ClientBuilder, SyncClientBuilder

async def example_async():
    async with ClientBuilder().error_for_status(True).build() as client:
        response = await client.get("https://httpbun.com/get").query({"q": "val"}).build().send()
        print(await response.json())        

def example_sync():
    with SyncClientBuilder().error_for_status(True).build() as client:
        print(client.get("https://httpbun.com/get").query({"q": "val"}).build().send().json())
```

Context manager usage is optional, but recommended. Also `close()` methods are available.

## Documentation

See [docs](https://markussintonen.github.io/pyreqwest/pyreqwest.html)

See [examples](https://github.com/MarkusSintonen/pyreqwest/tree/main/examples)
