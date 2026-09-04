<p align="center">
    <img width="250" alt="logo" src="https://raw.githubusercontent.com/MarkusSintonen/pyreqwest/refs/heads/main/docs/logo.png" />
</p>

---
[![codecov](https://codecov.io/github/markussintonen/pyreqwest/graph/badge.svg?token=OET0CMZYOH)](https://codecov.io/github/markussintonen/pyreqwest)
![PyPI - Python Version](https://img.shields.io/pypi/pyversions/pyreqwest)

pyreqwest - Powerful and fast Rust based HTTP client. Built on top of and inspired by [reqwest](https://github.com/seanmonstar/reqwest).

## Why

- No reinvention of the wheel - built on top of widely used reqwest and other Rust HTTP crates
- Secure and fast - no C-extension code, no Python code/dependencies, no `unsafe` code
- Ergonomic and easy to use - similar API as in reqwest, fully type-annotated
- Testing ergonomics - mocking included, can also connect into ASGI apps

Using this is a good choice when:

- You care about throughput and latency, especially in high concurrency scenarios
- You want a single solution to serve all your HTTP client needs

This is not a good choice when:

- You want a pure Python solution allowing debugging of the HTTP client internals
- You use alternative Python implementations or Python version older than 3.11

## Features

- High performance, see [notes](https://github.com/MarkusSintonen/pyreqwest/blob/main/docs/performance.md) and [benchmarks](https://github.com/MarkusSintonen/pyreqwest/blob/main/docs/benchmarks.md)
- Asynchronous and synchronous HTTP clients
- Customizable via middlewares and custom JSON serializers
- Ergonomic as `reqwest`
- HTTP/1.1 and HTTP/2 support (also HTTP/3 when it [stabilizes](https://docs.rs/reqwest/latest/reqwest/#unstable-features))
- Mocking and testing utilities (can also connect to ASGI apps)
- Fully type-safe with Python type hints
- Full test coverage
- Free threading, see [notes](https://github.com/MarkusSintonen/pyreqwest/blob/main/docs/performance.md#python-313-free-threading)

### Standard HTTP features you would expect

- HTTPS support (using [rustls](https://github.com/rustls/rustls))
- Request and response body streaming
- Connection pooling
- JSON, URLs, Headers, Cookies etc. (all serializers in Rust)
- Automatic decompression (zstd, gzip, brotli, deflate)
- Automatic response decoding (charset detection)
- Multipart form support
- Proxy support (also SOCKS)
- Redirects
- Timeouts
- Authentication (Basic, Bearer)
- Cookie management

## Quickstart

```python
# uv add pyreqwest

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

#### Mocking in pytest

```python
from pyreqwest.client import ClientBuilder
from pyreqwest.pytest_plugin import ClientMocker


async def test_client(client_mocker: ClientMocker) -> None:
    client_mocker.get(path="/api").with_body_text("Hello Mock")

    async with ClientBuilder().build() as client:
        response = await client.get("http://example.invalid/api").build().send()
        assert response.status == 200 and await response.text() == "Hello Mock"
        assert client_mocker.get_call_count() == 1
```

Manual mocking is available via `ClientMocker.create_mocker(MonkeyPatch)`.

#### Simple request interface

This is only recommended for simple use-cases such as scripts. Usually, the full client API should be used which reuses
connections and has other optimizations.

```python
# Sync example
from pyreqwest.simple.sync_request import pyreqwest_get

response = pyreqwest_get("https://httpbun.com/get").query({"q": "val"}).send()
print(response.json())
```

```python
# Async example
from pyreqwest.simple.request import pyreqwest_get

response = await pyreqwest_get("https://httpbun.com/get").query({"q": "val"}).send()
print(await response.json())
```

## Documentation

See [docs](https://markussintonen.github.io/pyreqwest/pyreqwest.html)

See [examples](https://github.com/MarkusSintonen/pyreqwest/tree/main/examples)

## Compatibility with other libraries

See [compatibility docs](https://markussintonen.github.io/pyreqwest/pyreqwest/compatibility.html)
