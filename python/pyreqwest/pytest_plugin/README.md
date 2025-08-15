# PyReqwest Pytest Plugin

A pytest plugin for mocking HTTP requests when using the PyReqwest HTTP client.

## Installation

The plugin is automatically available when you install `pyreqwest`. It will be auto-discovered by pytest through entry points.

## Usage

The plugin provides a `client_mocker` fixture that allows you to mock HTTP requests made by PyReqwest clients.

### Basic Example

```python
import pytest
from pyreqwest.client import ClientBuilder

async def test_api_call(client_mocker):
    # Mock a GET request
    client_mocker.get("https://api.example.com/users").json([
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"}
    ])
    
    # Use the client normally
    client = ClientBuilder().build()
    response = await client.get("https://api.example.com/users").build_consumed().send()
    
    # The request was mocked
    assert response.status == 200
    users = await response.json()
    assert len(users) == 2
    assert users[0]["name"] == "Alice"
```

### Advanced Features

#### Method-Specific Mocking

```python
async def test_crud_operations(client_mocker):
    client_mocker.get("https://api.example.com/users/123").json({"id": 123, "name": "John"})
    client_mocker.post("https://api.example.com/users").status(201).json({"id": 456, "name": "Jane"})
    client_mocker.put("https://api.example.com/users/123").status(204)
    client_mocker.delete("https://api.example.com/users/123").status(204)
    
    client = ClientBuilder().build()
    
    # All these requests will be mocked according to the rules above
    get_resp = await client.get("https://api.example.com/users/123").build_consumed().send()
    post_resp = await client.post("https://api.example.com/users").body_text('{"name": "Jane"}').build_consumed().send()
    # ... etc
```

#### URL Pattern Matching

```python
import re

async def test_regex_matching(client_mocker):
    # Mock any user ID
    pattern = re.compile(r"https://api\.example\.com/users/\d+")
    client_mocker.get(pattern).json({"id": "dynamic", "name": "Any User"})
    
    client = ClientBuilder().build()
    
    # Both of these will match
    resp1 = await client.get("https://api.example.com/users/123").build_consumed().send()
    resp2 = await client.get("https://api.example.com/users/456").build_consumed().send()
```

#### Header and Body Matching

```python
async def test_advanced_matching(client_mocker):
    # Match requests with specific headers
    client_mocker.mock(
        method="POST",
        url="https://api.example.com/secure",
        headers={"Authorization": "Bearer secret123"}
    ).status(200).json({"success": True})
    
    # Match requests with specific body content
    client_mocker.mock(
        method="POST",
        url="https://api.example.com/echo",
        body='{"action": "create"}'
    ).text("Action processed")
    
    client = ClientBuilder().build()
    
    # This will match the first rule
    auth_resp = await client.post("https://api.example.com/secure")\
        .header("Authorization", "Bearer secret123")\
        .build_consumed().send()
    
    # This will match the second rule
    echo_resp = await client.post("https://api.example.com/echo")\
        .body_text('{"action": "create"}')\
        .build_consumed().send()
```

#### Request Inspection

```python
async def test_request_capture(client_mocker):
    client_mocker.get("https://api.example.com/test").text("OK")
    
    client = ClientBuilder().build()
    await client.get("https://api.example.com/test")\
        .header("User-Agent", "MyApp/1.0")\
        .build_consumed().send()
    
    # Inspect captured requests
    requests = client_mocker.get_requests()
    assert len(requests) == 1
    assert requests[0].method == "GET"
    assert requests[0].headers.get("User-Agent") == "MyApp/1.0"
    
    # Count calls
    assert client_mocker.get_call_count() == 1
    assert client_mocker.get_call_count(method="GET") == 1
```

#### Response Types

```python
async def test_response_types(client_mocker):
    # Text response
    client_mocker.get("https://api.example.com/text").text("Hello World")
    
    # JSON response (automatically sets content-type header)
    client_mocker.get("https://api.example.com/json").json({"key": "value"})
    
    # Binary response
    client_mocker.get("https://api.example.com/binary").bytes(b"binary data")
    
    # Custom status and headers
    client_mocker.get("https://api.example.com/custom")\
        .status(201)\
        .text("Created")\
        .header("Location", "/resource/123")\
        .headers({"X-Rate-Limit": "100"})
```

#### Strict Mode

```python
async def test_strict_mode(client_mocker):
    client_mocker.strict(True)  # Only allow mocked requests
    client_mocker.get("https://api.example.com/allowed").text("OK")
    
    client = ClientBuilder().build()
    
    # This works
    resp = await client.get("https://api.example.com/allowed").build_consumed().send()
    
    # This raises AssertionError
    with pytest.raises(AssertionError, match="No mock rule matched request"):
        await client.get("https://api.example.com/forbidden").build_consumed().send()
```

#### Resetting Mocks

```python
async def test_mock_reset(client_mocker):
    client_mocker.get("https://api.example.com/test").text("response")
    
    client = ClientBuilder().build()
    await client.get("https://api.example.com/test").build_consumed().send()
    
    assert client_mocker.get_call_count() == 1
    
    # Reset all mocks and captured requests
    client_mocker.reset()
    
    assert client_mocker.get_call_count() == 0
    assert len(client_mocker.get_requests()) == 0
```

## API Reference

### ClientMocker

The main mocking class provided by the `client_mocker` fixture.

#### Methods

- `get(url)` / `post(url)` / `put(url)` / `delete(url)` / `patch(url)` / `head(url)` / `options(url)` - Mock specific HTTP methods
- `mock(method=None, url=None, headers=None, body=None)` - Generic mock with full control
- `strict(enabled=True)` - Enable/disable strict mode
- `get_requests(method=None, url=None)` - Get captured requests (optionally filtered)
- `get_call_count(method=None, url=None)` - Get call count (optionally filtered)
- `reset()` - Reset all mocks and captured data

### MockResponse

Response builder returned by mock methods.

#### Methods

- `status(code)` - Set HTTP status code
- `text(content)` - Set text response body
- `json(data)` - Set JSON response body (auto-sets content-type)
- `bytes(data)` - Set binary response body
- `header(name, value)` - Add a response header
- `headers(dict)` - Add multiple response headers

All methods return `self` for chaining.

### RequestMatcher

Used internally for matching requests. Supports:

- **Method matching**: Exact string match
- **URL matching**: Exact string or regex pattern
- **Header matching**: Exact string or regex pattern for header values
- **Body matching**: Exact string/bytes or regex pattern

## Plugin Registration

The plugin is automatically registered via setuptools entry points:

```toml
[project.entry-points.pytest11]
pyreqwest = "pyreqwest.pytest_plugin.plugin"
```

No manual registration or imports are required in your test files.
