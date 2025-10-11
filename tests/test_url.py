from copy import copy

import pytest
import yarl
from dirty_equals import Contains
from pyreqwest.http import Url


@pytest.mark.parametrize("kind", [Url, str, yarl.URL])
def test_init(kind: type):
    url = Url(kind("http://example.com"))
    assert url.scheme == "http"
    assert url.host_str == "example.com"


def test_parse():
    url = Url.parse("http://example.com")
    assert url.scheme == "http"
    assert url.host_str == "example.com"

    with pytest.raises(ValueError, match="invalid international domain name"):
        Url.parse("http://exa mple.com")
    with pytest.raises(ValueError, match="relative URL without a base"):
        Url.parse("/path")


def test_parse_with_params():
    url = Url.parse_with_params("http://example.com", {"key": "value"})
    assert url.scheme == "http"
    assert url.host_str == "example.com"
    assert url.query_string == "key=value"


def test_is_valid():
    assert Url.is_valid("http://example.com")
    assert Url.is_valid("http://example.com/path")
    assert not Url.is_valid("http://exa mple.com")
    assert not Url.is_valid("/path")
    assert not Url.is_valid("path")
    assert not Url.is_valid("")


def test_join():
    base_url = Url("http://example.com")
    joined_url = base_url.join("/path/to/resource")
    assert str(joined_url) == "http://example.com/path/to/resource"


def test_make_relative():
    base_url = Url("http://example.com/path/to/")
    relative_url = base_url.make_relative(Url("http://example.com/path/to/resource"))
    assert relative_url == "resource"


def test_origin_ascii():
    assert Url("http://example.com").origin_ascii == "http://example.com"
    assert Url("http://exämple.com").origin_ascii == "http://xn--exmple-cua.com"


def test_origin_unicode():
    assert Url("http://example.com").origin_unicode == "http://example.com"
    assert Url("http://exämple.com").origin_unicode == "http://exämple.com"


def test_scheme():
    assert Url("http://example.com").scheme == "http"
    assert Url("https://example.com").scheme == "https"


def test_is_special():
    assert Url("http://example.com").is_special is True
    assert Url("moz:///tmp/foo").is_special is False


def test_has_authority():
    assert Url("http://example.com").has_authority is True
    assert Url("unix:/run/foo.socket").has_authority is False


def test_authority():
    assert Url("http://example.com").authority == "example.com"
    assert Url("unix:/run/foo.socket").authority == ""


def test_cannot_be_a_base():
    assert Url("http://example.com").cannot_be_a_base is False
    assert Url("data:text/plain,Stuff").cannot_be_a_base is True


def test_username():
    assert Url("https://user:password@example.com/tmp/foo").username == "user"
    assert Url("https://user@example.com/tmp/foo").username == "user"
    assert Url("https://example.com/tmp/foo").username == ""


def test_password():
    assert Url("https://user:password@example.com/tmp/foo").password == "password"
    assert Url("https://user@example.com/tmp/foo").password is None
    assert Url("https://example.com/tmp/foo").password is None


def test_has_host():
    assert Url("http://example.com").has_host is True
    assert Url("unix:/run/foo.socket").has_host is False


def test_host_str():
    assert Url("http://example.com").host_str == "example.com"
    assert Url("http://127.0.0.1").host_str == "127.0.0.1"
    assert Url("unix:/run/foo.socket").host_str is None


def test_domain():
    assert Url("http://example.com").domain == "example.com"
    assert Url("http://127.0.0.1").domain is None
    assert Url("unix:/run/foo.socket").domain is None


def test_port():
    assert Url("http://example.com:8080").port == 8080
    assert Url("http://example.com").port is None


def test_port_or_known_default():
    assert Url("http://example.com:8080").port_or_known_default == 8080
    assert Url("http://example.com").port_or_known_default == 80
    assert Url("https://example.com").port_or_known_default == 443
    assert Url("unix:/run/foo.socket").port_or_known_default is None


def test_path():
    assert Url("http://example.com/path/to/resource/").path == "/path/to/resource/"
    assert Url("http://example.com/path/to/resource").path == "/path/to/resource"
    assert Url("http://example.com/pa th").path == "/pa%20th"
    assert Url("http://example.com/").path == "/"
    assert Url("http://example.com").path == "/"
    assert Url("data:text/plain,HelloWorld").path == "text/plain,HelloWorld"


def test_path_segments():
    assert Url("http://example.com/path/to/resource/").path_segments == ["path", "to", "resource", ""]
    assert Url("http://example.com/path/to/resource").path_segments == ["path", "to", "resource"]
    assert Url("http://example.com/path/").path_segments == ["path", ""]
    assert Url("http://example.com/path").path_segments == ["path"]
    assert Url("http://example.com/pa th").path_segments == ["pa%20th"]
    assert Url("http://example.com/").path_segments == [""]
    assert Url("http://example.com").path_segments == [""]
    assert Url("data:text/plain,HelloWorld").path_segments is None


def test_query_string():
    url = Url.parse_with_params("http://example.com", {"key": "value"})
    assert url.query_string == "key=value"

    url = Url.parse_with_params("http://example.com", {"key1": "value1", "key2": "value2"})
    assert url.query_string == "key1=value1&key2=value2"

    assert Url("http://example.com").query_string is None


def test_query_pairs():
    url = Url.parse_with_params("http://example.com", {"key": "value"})
    assert url.query_pairs == [("key", "value")]

    url = Url.parse_with_params("http://example.com", {"key1": "value1", "key2": "value2"})
    assert url.query_pairs == [("key1", "value1"), ("key2", "value2")]

    assert Url("http://example.com").query_pairs == []


def test_query_dict_multi_value():
    url = Url.parse_with_params("http://example.com", {"key": "value"})
    assert url.query_dict_multi_value == {"key": "value"}

    url = Url.parse_with_params("http://example.com", [("key1", "value1"), ("key1", "value2")])
    assert url.query_dict_multi_value == {"key1": ["value1", "value2"]}

    url = Url.parse_with_params("http://example.com", [("key1", "value1"), ("key1", "value2"), ("key1", "value3")])
    assert url.query_dict_multi_value == {"key1": ["value1", "value2", "value3"]}

    url2 = Url.parse_with_params("http://example.com", url.query_dict_multi_value)
    assert url2.query_dict_multi_value == url.query_dict_multi_value

    assert Url("http://example.com").query_dict_multi_value == {}


def test_fragment():
    assert Url("http://example.com#section1").fragment == "section1"
    assert Url("http://example.com").fragment is None


def test_with_fragment():
    url = Url("http://example.com")
    assert str(url.with_fragment("section1")) == "http://example.com/#section1"
    assert str(url.with_fragment(None)) == "http://example.com/"


def test_with_query():
    url = Url("http://example.com")
    assert str(url.with_query({"key": "value"})) == "http://example.com/?key=value"
    assert str(url.with_query({"key": 1})) == "http://example.com/?key=1"
    assert str(url.with_query({"key": True})) == "http://example.com/?key=true"
    assert str(url.with_query([("key", "value")])) == "http://example.com/?key=value"
    assert str(url.with_query([("k1", "v1"), ("k1", "v2")])) == "http://example.com/?k1=v1&k1=v2"
    assert str(url.with_query({"k1": ["v1", "v2"]})) == "http://example.com/?k1=v1&k1=v2"
    assert str(url) == "http://example.com/"

    url = Url("http://example.com/?key=value")
    assert str(url.with_query({"key2": "value2"})) == "http://example.com/?key2=value2"
    assert str(url.with_query(None)) == "http://example.com/"
    assert str(url) == "http://example.com/?key=value"


def test_extend_query():
    url = Url("http://example.com")
    url2 = url.extend_query({"key1": "value1"})
    url3 = url2.extend_query({"key2": "value2"})
    assert str(url3) == "http://example.com/?key1=value1&key2=value2"
    assert str(url2) == "http://example.com/?key1=value1"
    assert str(url) == "http://example.com/"


def test_with_query_string():
    url = Url("http://example.com?key=value")
    assert str(url.with_query_string("key2=value2")) == "http://example.com/?key2=value2"
    assert str(url.with_query_string(None)) == "http://example.com/"
    assert str(url) == "http://example.com/?key=value"


def test_with_path():
    url = Url("http://example.com/old")
    assert str(url.with_path("/new/path")) == "http://example.com/new/path"
    assert str(url.with_path("new/path")) == "http://example.com/new/path"
    assert str(url.with_path("/")) == "http://example.com/"
    assert str(url.with_path("")) == "http://example.com/"
    assert str(url) == "http://example.com/old"


def test_with_path_segments():
    url = Url("http://example.com/old")
    assert str(url.with_path_segments(["new", "path"])) == "http://example.com/new/path"
    assert str(url.with_path_segments(["new", "path", ""])) == "http://example.com/new/path/"
    assert str(url.with_path_segments([])) == "http://example.com/"
    assert str(url) == "http://example.com/old"


def test_with_port():
    url = Url("http://example.com:1234")
    assert str(url.with_port(8080)) == "http://example.com:8080/"
    assert str(url.with_port(None)) == "http://example.com/"
    assert str(url) == "http://example.com:1234/"


def test_with_host():
    url = Url("http://example.com")
    assert str(url.with_host("newhost.com")) == "http://newhost.com/"
    with pytest.raises(ValueError, match="empty host"):
        url.with_host(None)
    with pytest.raises(ValueError, match="empty host"):
        url.with_host("")
    assert str(url) == "http://example.com/"


def test_with_ip_host():
    url = Url("http://example.com")
    assert str(url.with_ip_host("127.0.0.1")) == "http://127.0.0.1/"
    assert str(url) == "http://example.com/"


def test_with_username():
    url = Url("http://example.com")
    assert str(url.with_username("user")) == "http://user@example.com/"
    assert str(url) == "http://example.com/"


def test_with_password():
    url = Url("http://example.com")
    assert str(url.with_password("pass")) == "http://:pass@example.com/"
    assert str(url) == "http://example.com/"


def test_with_scheme():
    url = Url("http://example.com")
    assert str(url.with_scheme("https")) == "https://example.com/"

    with pytest.raises(ValueError, match="Invalid scheme"):
        url.with_scheme("foobar")
    assert str(url) == "http://example.com/"


def test_copy():
    url = Url("http://example.com")
    url2 = copy(url)
    assert url2 is not url
    assert url2 == url


def test_div_join():
    url = Url("http://example.com")
    assert str(url / "path/to/resource") == "http://example.com/path/to/resource"
    assert str(url / "/path/to/resource") == "http://example.com/path/to/resource"
    assert str(url / "path/to/resource/") == "http://example.com/path/to/resource/"
    assert str(url / "path") == "http://example.com/path"
    assert str(url / "") == "http://example.com/"
    assert str(url) == "http://example.com/"


def test_str():
    url = Url("http://example.com/path/to/resource?key=value#section1")
    assert str(url) == "http://example.com/path/to/resource?key=value#section1"


def test_repr():
    url = Url("http://example.com/path/to/resource?key=value#section1")
    assert repr(url) == "Url('http://example.com/path/to/resource?key=value#section1')"


def test_hash():
    url1 = Url("http://example1.com")
    url2 = Url("http://example1.com/")
    url3 = Url("http://example2.com")
    d = {url1: "ex1", url2: "ex2", url3: "ex3"}
    assert [*d.values()] == ["ex2", "ex3"]
    assert d[url1] == "ex2"
    assert d[url2] == "ex2"
    assert d[url3] == "ex3"
    assert d.get(Url("http://example3.com")) is None


@pytest.mark.parametrize("kind", [Url, str, yarl.URL])
def test_eq(kind: type):
    url1 = Url("http://example.com")
    url2 = Url("http://example.com/")
    url3 = Url("http://example.org")
    assert url1 == kind(str(url1))
    assert url1 == kind(str(url2))
    assert url1 != kind(str(url3))
    assert url2 != kind(str(url3))
    assert not (url1 != kind("http://example.com"))
    assert Url("http://example.com") == Contains("example")


def test_cmp():
    url1 = Url("http://a.example.com")
    url2 = Url("http://b.example.com")
    assert url1 < url2 and url1 <= url2
    assert url2 > url1 and url2 >= url1


@pytest.mark.parametrize("url_str", ["http://example.com/path", "http://example.com/path/"])
def test_sequence_dunder(url_str: str):
    url = Url(url_str)
    assert len(url) == len(url_str)
    assert "http://" in url and "example.com" in url and "/path" in url

    for i in range(len(url)):
        assert url[i] == url_str[i]
    with pytest.raises(IndexError):
        _ = url[len(url) + 1]
    assert url[:5] == url_str[:5]

    assert list(iter(url)) == list(iter(url_str))
