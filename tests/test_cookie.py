from collections.abc import Sequence
from copy import copy
from datetime import UTC, datetime, timedelta

import pytest
from pyreqwest.client import ClientBuilder
from pyreqwest.http.cookie import Cookie, CookieStore

from tests.servers.server_subprocess import SubprocessServer


def client_builder() -> ClientBuilder:
    return ClientBuilder().error_for_status(True).timeout(timedelta(seconds=5))


async def test_cookie_provider(echo_server: SubprocessServer):
    assert echo_server.url.host_str
    store = CookieStore()

    async with client_builder().cookie_provider(store).build() as client:
        url1 = (echo_server.url / "path1").with_query({"header_Set_Cookie": "name1=val1"})
        await client.get(url1).build().send()

        url2 = (echo_server.url / "path2").with_query({"header_Set_Cookie": "name2=val2; Path=/path2"})
        await client.get(url2).build().send()

    url3 = echo_server.url / "path3"
    store.insert("name3=val3; Path=/path3", url3)

    assert store.matches(echo_server.url) == ["name1=val1"]
    assert store.matches(url1) == ["name1=val1"]
    assert store.matches(url2) == ["name1=val1", "name2=val2; Path=/path2"]
    assert store.matches(url3) == ["name1=val1", "name3=val3; Path=/path3"]

    assert store.contains(domain=echo_server.url.host_str, path="/path3", name="name3") is True
    assert store.contains_any(domain=echo_server.url.host_str, path="/path3", name="name3") is True

    assert store.get(domain=echo_server.url.host_str, path="/path2", name="name2") == "name2=val2; Path=/path2"
    assert store.get_any(domain=echo_server.url.host_str, path="/path3", name="name3") == "name3=val3; Path=/path3"
    assert store.get_all_unexpired() == ["name1=val1", "name2=val2; Path=/path2", "name3=val3; Path=/path3"]
    assert store.get_all_any() == ["name1=val1", "name2=val2; Path=/path2", "name3=val3; Path=/path3"]

    assert store.remove(domain=echo_server.url.host_str, path="/path3", name="unknown") is None
    assert store.remove(domain=echo_server.url.host_str, path="/path3", name="name3") == "name3=val3; Path=/path3"
    assert store.get_all_unexpired() == ["name1=val1", "name2=val2; Path=/path2"]
    assert store.get_all_any() == ["name1=val1", "name2=val2; Path=/path2"]

    store.clear()
    assert store.get_all_any() == []


def test_cookie_create():
    assert str(Cookie("key", "val")) == "key=val"
    assert str(Cookie.parse("key=val")) == "key=val"
    assert repr(Cookie.parse("key=val")) == "Cookie('key=val')"
    assert str(Cookie.parse("key=val; Path=/foo; HttpOnly")) == "key=val; HttpOnly; Path=/foo"
    assert str(Cookie.parse_encoded("key=val%20with%20spaces")) == "key=val with spaces"
    assert Cookie.parse_encoded("key=val%20with%20spaces").encode() == "key=val%20with%20spaces"
    assert Cookie.split_parse("key1=val1; key2=val2") == ["key1=val1", "key2=val2"]
    assert Cookie.split_parse_encoded("key1=val1; key2=val%202") == ["key1=val1", "key2=val 2"]


def test_cookie_attrs():
    c = Cookie.parse(
        "key=val; Path=/foo; HttpOnly; Secure; SameSite=Strict; Partitioned; Domain=foo.invalid;"
        " Expires=Wed, 09 Jun 2025 10:18:14 GMT; Max-Age=3600"
    )
    assert c.name == "key"
    assert c.value == "val"
    assert c.value_trimmed == "val"
    assert c.path == "/foo"
    assert c.http_only is True
    assert c.secure is True
    assert c.same_site == "Strict"
    assert c.expires_datetime == datetime(2025, 6, 9, 10, 18, 14, tzinfo=UTC)
    assert c.max_age == timedelta(hours=1)
    assert c.stripped() == "key=val"
    assert c.partitioned is True
    assert c.domain == "foo.invalid"
    assert Cookie.parse("key=val").expires_datetime is None
    assert Cookie.parse("key=val").same_site is None
    assert Cookie.parse("key=val").partitioned is False
    assert Cookie.parse("key=val").domain is None


def test_cookie_hash_eq():
    class CookieLike:
        def __str__(self) -> str:
            return "key=val; Path=/foo; HttpOnly"

    c1 = Cookie.parse("key=val; Path=/foo; HttpOnly")
    c2 = Cookie.parse("key=val; HttpOnly; Path=/foo")
    c3 = Cookie.parse("key=val; Path=/bar; HttpOnly")
    assert sorted([str(c) for c in {c1, c2, c3}]) == ["key=val; HttpOnly; Path=/bar", "key=val; HttpOnly; Path=/foo"]
    assert hash(c1) == hash(c2)
    assert hash(c1) != hash(c3)
    assert c1 == c2
    assert c1 == str(c2)
    assert c1 == CookieLike()
    assert c1 != c3
    assert c1 != str(c3)
    assert c1 != "not a cookie"
    assert c1 != 1
    assert copy(c1) == c1 and copy(c1) is not c1


def test_cookie_sequence():
    c = Cookie.parse("key=val;Path=/foo;HttpOnly")
    str_c = str(c)
    assert str_c == "key=val; HttpOnly; Path=/foo"
    assert c == str_c
    assert type(c) is Cookie and isinstance(c, Sequence)
    assert len(c) == len(str_c)
    assert "HttpOnly" in c
    assert all(c[i] == str_c[i] for i in range(len(c)))
    assert [*iter(c)] == [*str_c]
    assert [*reversed(c)] == [*reversed(str_c)]
    assert c.index("HttpOnly") == str_c.index("HttpOnly")
    assert c.count("HttpOnly") == str_c.count("HttpOnly")


def test_cookie_with_changes():
    cookie = Cookie("key", "val")
    cookie2 = (
        cookie.with_value("newval")
        .with_http_only(True)
        .with_secure(True)
        .with_same_site("Lax")
        .with_partitioned(True)
        .with_max_age(timedelta(minutes=10))
        .with_path("/foo")
        .with_domain("example.com")
        .with_expires_datetime(datetime(2025, 6, 9, 10, 18, 14, tzinfo=UTC))
    )
    cookie3 = cookie2.with_name("newkey")
    assert str(cookie) == "key=val"
    assert str(cookie2) == (
        "key=newval; HttpOnly; SameSite=Lax; Partitioned; Secure; Path=/foo;"
        " Domain=example.com; Max-Age=600; Expires=Mon, 09 Jun 2025 10:18:14 GMT"
    )
    assert str(cookie3) == (
        "newkey=newval; HttpOnly; SameSite=Lax; Partitioned; Secure; Path=/foo;"
        " Domain=example.com; Max-Age=600; Expires=Mon, 09 Jun 2025 10:18:14 GMT"
    )


def test_cookie__with_same_site():
    assert Cookie("k", "v").with_same_site("Strict").same_site == "Strict"
    assert Cookie("k", "v").with_same_site("Lax").same_site == "Lax"
    assert Cookie("k", "v").with_same_site("None").same_site == "None"

    with pytest.raises(ValueError, match="invalid SameSite"):
        Cookie("k", "v").with_same_site("invalid")  # type: ignore[arg-type]


def test_cookie__with_path():
    assert Cookie("k", "v").with_path("/foo").path == "/foo"
    assert Cookie("k", "v").with_path("/foo").with_path(None).path is None


def test_cookie__with_domain():
    assert Cookie("k", "v").with_domain("example.com").domain == "example.com"
    assert Cookie("k", "v").with_domain("example.com").with_domain(None).domain is None
