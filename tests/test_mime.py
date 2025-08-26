from collections.abc import Sequence
from copy import copy

import pytest
from dirty_equals import Contains
from pyreqwest.http import Mime


def test_mime():
    mime = Mime.parse("text/plain")
    assert mime.type_ == "text"
    assert mime.subtype == "plain"
    assert mime.suffix is None
    assert mime.parameters == []
    assert mime.get_param("charset") is None

    mime = Mime.parse("application/json; charset=utf-8")
    assert mime.type_ == "application"
    assert mime.subtype == "json"
    assert mime.suffix is None
    assert mime.parameters == [("charset", "utf-8")]
    assert mime.get_param("charset") == "utf-8"

    mime = Mime.parse("multipart/form-data; boundary=----FooBar")
    assert mime.type_ == "multipart"
    assert mime.subtype == "form-data"
    assert mime.suffix is None
    assert mime.parameters == [("boundary", "----FooBar")]

    mime = Mime.parse("image/svg+xml")
    assert mime.type_ == "image"
    assert mime.subtype == "svg"
    assert mime.suffix == "xml"
    assert mime.parameters == []


def test_eq():
    mime1 = Mime.parse("text/plain")
    mime2 = Mime.parse("text/plain")
    mime3 = Mime.parse("application/json")

    assert mime1 == mime2
    assert mime1 != mime3
    assert mime2 != mime3

    mime4 = Mime.parse("text/plain; charset=utf-8")
    mime5 = Mime.parse("text/plain;charset=utf-8")
    assert mime4 == mime5
    assert mime1 != mime4


def test_eq_support():
    assert Mime.parse("application/json") == Contains("json")


def test_copy():
    mime = Mime.parse("text/plain")
    assert copy(mime) is not mime
    assert copy(mime) == mime


def test_str():
    assert str(Mime.parse("text/plain; charset=utf-8")) == "text/plain; charset=utf-8"
    assert str(Mime.parse("application/json")) == "application/json"
    assert str(Mime.parse("image/svg+xml")) == "image/svg+xml"


def test_repr():
    assert repr(Mime.parse("text/plain; charset=utf-8")) == "Mime('text/plain; charset=utf-8')"
    assert repr(Mime.parse("application/json")) == "Mime('application/json')"
    assert repr(Mime.parse("image/svg+xml")) == "Mime('image/svg+xml')"


def test_hash():
    mime1 = Mime.parse("text/plain")
    mime2 = Mime.parse("text/plain;")
    mime3 = Mime.parse("application/json")
    d = {mime1: "text1", mime2: "text2", mime3: "json"}
    assert [*d.values()] == ["text2", "json"]
    assert d[Mime.parse("text/plain")] == "text2"
    assert d[Mime.parse("text/plain;")] == "text2"
    assert d[Mime.parse("application/json")] == "json"
    assert d.get(Mime.parse("application/json; charset=utf-8")) is None


@pytest.mark.parametrize(
    "mime_str",
    ["application/json", "application/json; charset=utf-8", "application/json;charset=utf-8"],
)
def test_sequence(mime_str: str):
    mime = Mime.parse(mime_str)
    assert len(mime) == len(mime_str)
    assert "application" in mime and "/json" in mime

    for i in range(len(mime)):
        assert mime[i] == mime_str[i]
    with pytest.raises(IndexError):
        _ = mime[len(mime) + 1]
    assert mime[:5] == mime_str[:5]

    assert list(iter(mime)) == list(iter(mime_str))
    assert list(reversed(mime)) == list(reversed(mime_str))

    assert mime.index("json") == mime_str.index("json")
    assert mime.count("/") == mime_str.count("/")


def test_abc():
    assert isinstance(Mime.parse("text/plain"), Mime)
    assert isinstance(Mime.parse("text/plain"), Sequence)
    assert not isinstance(Mime.parse("text/plain"), str)
    assert issubclass(Mime, Mime)
    assert issubclass(Mime, Sequence)
    assert not issubclass(Mime, str)
