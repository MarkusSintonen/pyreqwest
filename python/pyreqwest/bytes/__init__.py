"""Buffer protocol implementation."""

from pyreqwest._pyreqwest.bytes import Bytes

Bytes.__doc__ = """A `bytes`-like buffer.

This implements the Python buffer protocol, allowing zero-copy access
to underlying Rust memory.

You can pass this to `memoryview` for a zero-copy view into the underlying
data or to `bytes` to copy the underlying data into a Python `bytes`.

Many methods from the Python `bytes` class are implemented on this,
"""

__all__ = ["Bytes"]
