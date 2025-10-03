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
    assert mime.essence_str == "text/plain"

    mime = Mime.parse("application/json; charset=utf-8")
    assert mime.type_ == "application"
    assert mime.subtype == "json"
    assert mime.suffix is None
    assert mime.parameters == [("charset", "utf-8")]
    assert mime.get_param("charset") == "utf-8"
    assert mime.essence_str == "application/json"

    mime = Mime.parse("multipart/form-data; boundary=----FooBar")
    assert mime.type_ == "multipart"
    assert mime.subtype == "form-data"
    assert mime.suffix is None
    assert mime.parameters == [("boundary", "----FooBar")]
    assert mime.essence_str == "multipart/form-data"

    mime = Mime.parse("image/svg+xml")
    assert mime.type_ == "image"
    assert mime.subtype == "svg"
    assert mime.suffix == "xml"
    assert mime.parameters == []
    assert mime.essence_str == "image/svg+xml"


def test_eq():
    mime = Mime.parse("text/plain")
    assert mime == Mime.parse("text/plain")
    assert mime == "text/plain"
    assert mime != Mime.parse("application/json")
    assert mime != "application/json"

    mime2 = Mime.parse("text/plain; charset=utf-8")
    assert mime2 == Mime.parse("text/plain;charset=utf-8")
    assert mime2 == "text/plain;charset=utf-8"
    assert mime2 != mime

    assert Mime.parse("application/json") == Contains("json")


def test_cmp():
    mime1 = Mime.parse("text/plain")
    mime2 = Mime.parse("text/plain; charset=utf-8")
    assert mime1 < mime2 and mime1 <= mime2
    assert mime2 > mime1 and mime2 >= mime1


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
def test_sequence_dunder(mime_str: str):
    mime = Mime.parse(mime_str)
    assert len(mime) == len(mime_str)
    assert "application" in mime and "/json" in mime

    for i in range(len(mime)):
        assert mime[i] == mime_str[i]
    with pytest.raises(IndexError):
        _ = mime[len(mime) + 1]
    assert mime[:5] == mime_str[:5]

    assert list(iter(mime)) == list(iter(mime_str))
