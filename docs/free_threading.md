### Python 3.13+ free threading

Library supports Python 3.13+ free threading.

Following classes are thread-safe to use across multiple threads: `Client`, `SyncClient`, `CookieStore`.
These do not require additional locking or synchronization. Multiple requests can be started by different threads
concurrently.

Also, simple types and immutable types like `Url`, `HeaderMap`, `Bytes`, `Mime`, `Cookie` are thread-safe.

Builder classes are not thread-safe and should not be shared across threads.
(For example `ClientBuilder` and `SyncClientBuilder`.)
Multiple threads should mutate the same builder object concurrently.

Also, request and response types are not thread-safe.
(For example `ConsumedRequest`, `Response`, `SyncConsumedRequest`, `SyncResponse`.)
Multiple threads should not read or write to the same request or response object concurrently.
