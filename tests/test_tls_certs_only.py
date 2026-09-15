"""Exercise strict root selection against a local HTTPS server."""

import os
import ssl
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import trustme


def test_tls_certs_only(tmp_path: Path) -> None:
    ca = trustme.CA()
    cert = ca.issue_cert("localhost")
    ca_path = tmp_path / "ca.pem"
    ca.cert_pem.write_to_path(ca_path)
    empty_cert_dir = tmp_path / "empty-cert-dir"
    empty_cert_dir.mkdir()

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            self.send_response(200)
            self.send_header("Content-Length", "2")
            self.end_headers()
            self.wfile.write(b"ok")

        def log_message(self, *args: object) -> None:
            pass

    server = ThreadingHTTPServer(("localhost", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    cert.configure_cert(context)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        # Fresh subprocesses avoid cached platform-verifier roots. Linux lets
        # us inject a native root via SSL_CERT_FILE; macOS and Windows use
        # platform stores. Explicit-root success and rejection run everywhere.
        script = """
import sys
from pathlib import Path
from pyreqwest.client import SyncClientBuilder
from pyreqwest.exceptions import ConnectError
builder = SyncClientBuilder().no_proxy()
if sys.argv[1] != 'native':
    builder = builder.tls_certs_only()
if sys.argv[1] == 'explicit':
    builder = builder.add_root_certificate_pem(Path(sys.argv[2]).read_bytes())
with builder.build() as client:
    try:
        response = client.get(sys.argv[3]).build().send()
    except ConnectError:
        assert sys.argv[1] == 'empty', 'Expected the configured root to be trusted'
    else:
        assert sys.argv[1] != 'empty', 'Native roots remained trusted'
        assert bytes(response.bytes()) == b'ok'
"""
        env = {**os.environ, "SSL_CERT_FILE": str(ca_path), "SSL_CERT_DIR": str(empty_cert_dir)}
        modes = ("native", "empty", "explicit") if sys.platform == "linux" else ("empty", "explicit")
        for mode in modes:
            subprocess.run(  # noqa: S603 - fixed test script and locally generated paths
                [sys.executable, "-c", script, mode, str(ca_path), f"https://localhost:{server.server_port}"],
                env=env,
                check=True,
                capture_output=True,
                text=True,
                timeout=15,
            )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
