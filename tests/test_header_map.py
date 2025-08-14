import re
from collections.abc import MutableMapping, Mapping, ItemsView, KeysView, ValuesView
from copy import copy
from typing import Callable, Any

import pytest
from multidict import CIMultiDict

from pyreqwest.http import HeaderMap, HeaderMapItemsView, HeaderMapKeysView, HeaderMapValuesView


def test_init__empty():
    assert len(HeaderMap()) == 0


@pytest.mark.parametrize(
    "pairs", [[], [("a", "v1")], [("a", "v1"), ("b", "v2")], [("a", "v1"), ("b", "v2"), ("a", "v3")]]
)
@pytest.mark.parametrize("kind", [list, tuple, dict, CIMultiDict, HeaderMap])
def test_init__args(pairs: list[tuple[str, str]], kind: Callable[[list], Any]):
    headers = HeaderMap(kind(pairs))
    if kind == dict:
        assert len(headers) == len(dict(pairs))
        assert headers == dict(pairs)
    else:
        assert len(headers) == len(pairs)
        assert headers == CIMultiDict(pairs)


def test_init__bad():
    with pytest.raises(TypeError, match="'str' object cannot be converted"):
        HeaderMap("invalid")
    with pytest.raises(TypeError, match="'int' object cannot be converted to 'Mapping'"):
        HeaderMap(1)
    with pytest.raises(TypeError, match="'str' object cannot be converted"):
        HeaderMap(["a"])
    with pytest.raises(TypeError, match="'int' object cannot be converted to 'PyString'"):
        HeaderMap({"a": 1})
    with pytest.raises(ValueError, match="failed to parse header value"):
        HeaderMap({"a": "a\n"})
    with pytest.raises(ValueError, match="invalid HTTP header name"):
        HeaderMap({"a\n": "a"})


def test_getitem():
    with pytest.raises(KeyError, match="'a'"):
        _ = HeaderMap()["a"]

    headers = HeaderMap([("a", "v1"), ("a", "v2"), ("b", "v3")])
    assert headers["a"] == "v1"
    assert headers["b"] == "v3"
    with pytest.raises(KeyError, match="'c'"):
        _ = headers["c"]


def test_setitem():
    headers = HeaderMap()
    headers["a"] = "b"
    assert len(headers) == 1
    assert headers["a"] == "b"

    headers["c"] = "d"
    assert len(headers) == 2
    assert headers["c"] == "d"

    headers["a"] = "e"
    assert len(headers) == 2
    assert headers["a"] == "e"

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers["a\n"] = "f"
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers["a"] = "f\n"


def test_delitem():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    with pytest.raises(KeyError, match=re.escape("'a\\n'")):
        del headers["a\n"]
    with pytest.raises(KeyError, match="'c'"):
        del headers["c"]
    assert len(headers) == 3

    del headers["a"]
    assert len(headers) == 1
    with pytest.raises(KeyError, match="'a'"):
        _ = headers["a"]

    del headers["b"]
    assert len(headers) == 0
    with pytest.raises(KeyError, match="'b'"):
        _ = headers["b"]


@pytest.mark.parametrize(
    "pairs", [[], [("a", "v1")], [("a", "v1"), ("b", "v2")], [("a", "v1"), ("b", "v2"), ("a", "v3")]]
)
def test_iter(pairs: list[tuple[str, str]]):
    headers = HeaderMap(pairs)
    assert sorted(list(iter(headers))) == sorted([k for k, _ in pairs])
    assert {**headers} == {k: v for k, v in reversed(pairs)} # In HeaderMap first one wins


def test_bool():
    assert not HeaderMap()
    assert not HeaderMap([])
    assert HeaderMap([("a", "v1")])


@pytest.mark.parametrize(
    "pairs", [[], [("a", "v1")], [("a", "v1"), ("b", "v2")], [("a", "v1"), ("b", "v2"), ("a", "v3")]]
)
def test_len(pairs: list[tuple[str, str]]):
    headers = HeaderMap(pairs)
    assert len(headers) == len(pairs)
    assert len(headers) == len(CIMultiDict(pairs))
    assert headers.len() == len(pairs)
    assert headers.keys_len() == len({k for k, _ in pairs})


def test_contains():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert "a" in headers
    assert "b" in headers
    assert "c" not in headers
    assert "A" in headers
    assert "B" in headers


def test_items():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    items = headers.items()
    assert len(items) == 3
    assert sorted([*items]) == [("a", "v1"), ("a", "v3"), ("b", "v2")]

    headers["c"] = "v4"
    assert len(items) == 4
    assert sorted([*items]) == [("a", "v1"), ("a", "v3"), ("b", "v2"), ("c", "v4")]

    assert ("b", "v2") in items
    assert ("b", "v1") not in items
    assert ("d", "v1") not in items

    assert sorted(reversed(items)) == [("a", "v1"), ("a", "v3"), ("b", "v2"), ("c", "v4")]  # Ill-defined

    assert items == [("a", "v1"), ("a", "v3"), ("b", "v2"), ("c", "v4")]
    assert items == [("c", "v4"), ("a", "v1"), ("a", "v3"), ("b", "v2")]
    assert items == {("c", "v4"), ("a", "v1"), ("a", "v3"), ("b", "v2")}
    assert items != [("a", "v1"), ("a", "v3"), ("b", "v3"), ("c", "v4")]
    assert items != [("a", "v1"), ("a", "v3"), ("c", "v4")]
    assert items != []
    assert items != ""

    assert str(HeaderMap([("a", "v1")]).items()) == "[('a', 'v1')]"
    assert repr(HeaderMap([("a", "v1")]).items()) == "HeaderMapItemsView([('a', 'v1')])"


def test_keys():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    keys = headers.keys()
    assert len(keys) == 3
    assert sorted([*keys]) == ["a", "a", "b"]

    headers["c"] = "v4"
    assert len(keys) == 4
    assert sorted([*keys]) == ["a", "a", "b", "c"]

    assert "b" in keys
    assert "d" not in keys

    assert sorted(reversed(keys)) == ["a", "a", "b", "c"]  # Ill-defined

    assert keys == ["a", "a", "b", "c"]
    assert keys == ["a", "a", "c", "b"]
    assert keys != {"a", "c", "b"}
    assert keys != ["a", "b"]
    assert keys != []
    assert keys != ""

    assert str(HeaderMap([("a", "v1")]).keys()) == "['a']"
    assert repr(HeaderMap([("a", "v1")]).keys()) == "HeaderMapKeysView(['a'])"


def test_values():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    values = headers.values()
    assert len(values) == 3
    assert sorted([*values]) == ["v1", "v2", "v3"]

    headers["c"] = "v4"
    assert len(values) == 4
    assert sorted([*values]) == ["v1", "v2", "v3", "v4"]

    assert "v2" in values
    assert "v5" not in values

    assert sorted(reversed(values)) == ["v1", "v2", "v3", "v4"]  # Ill-defined

    assert str(HeaderMap([("a", "v1")]).values()) == "['v1']"
    assert repr(HeaderMap([("a", "v1")]).values()) == "HeaderMapValuesView(['v1'])"


def test_get():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert headers.get("a") == "v1"
    assert headers.get("a", "b\n") == "v1"
    assert headers.get("a\n") is None
    assert headers.get("b") == "v2"
    assert headers.get("c") is None
    assert headers.get("c", "default") == "default"
    assert headers.get("c", 1) == 1
    assert headers.get("c", "b\n") == "b\n"
    assert HeaderMap().get("a") is None


def test_eq():
    pairs = [("a", "v1"), ("b", "v2"), ("a", "v3")]
    headers = HeaderMap(pairs)
    assert headers == HeaderMap(pairs)
    assert headers == HeaderMap([("a", "v1"), ("a", "v3"), ("b", "v2")])
    assert headers == CIMultiDict(pairs)
    assert HeaderMap({"a": "v1", "b": "v2"}) == {"b": "v2", "a": "v1"}
    assert not (headers == HeaderMap())
    assert not (headers == HeaderMap([("a", "v1"), ("b", "v2")]))
    assert not (headers == HeaderMap([("a", "v1"), ("b", "v2"), ("c", "v3")]))
    assert not (headers == HeaderMap([("a", "v2"), ("a", "v3"), ("b", "v2")]))
    assert not (headers == HeaderMap([*reversed(pairs)]))
    assert not (headers == {"a": "v1", "b": "v2"})
    assert not (headers == [("a", "v1"), ("b", "v2")])
    assert not (headers == pairs)
    assert not (headers == [])
    assert not (headers == "")
    assert not (headers == 123)


def test_ne():
    pairs = [("a", "v1"), ("b", "v2"), ("a", "v3")]
    headers = HeaderMap(pairs)
    assert not (headers != HeaderMap(pairs))
    assert not (headers != HeaderMap([("a", "v1"), ("a", "v3"), ("b", "v2")]))
    assert not (headers != CIMultiDict(pairs))
    assert not (HeaderMap({"a": "v1", "b": "v2"}) != {"b": "v2", "a": "v1"})
    assert headers != HeaderMap()
    assert headers != HeaderMap([("a", "v1"), ("b", "v2")])
    assert headers != HeaderMap([("a", "v1"), ("b", "v2"), ("c", "v3")])
    assert headers != HeaderMap([("a", "v2"), ("a", "v3"), ("b", "v2")])
    assert headers != HeaderMap([*reversed(pairs)])
    assert headers != {"a": "v1", "b": "v2"}
    assert headers != [("a", "v1"), ("b", "v2")]
    assert headers != pairs
    assert headers != []
    assert headers != ""
    assert headers != 123


def test_pop():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    with pytest.raises(KeyError, match="'c'"):
        headers.pop("c")
    with pytest.raises(KeyError, match=re.escape("'a\\n'")):
        headers.pop("a\n")
    assert len(headers) == 3

    assert headers.pop("a") == "v1"
    assert len(headers) == 2
    assert headers.pop("b") == "v2"
    assert len(headers) == 1
    assert headers.pop("a") == "v3"
    assert len(headers) == 0

    with pytest.raises(KeyError, match="'a'"):
        headers.pop("a")

    assert headers.pop("d", default=None) is None
    assert headers.pop("d", "default") == "default"
    assert headers.pop("d", 1) == 1


def test_popitem():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    popped = []
    popped.append(headers.popitem())
    assert len(headers) == 2
    popped.append(headers.popitem())
    assert len(headers) == 1
    popped.append(headers.popitem())
    assert len(headers) == 0
    assert [kv for kv in popped if kv[0] == "a"] == [("a", "v1"), ("a", "v3")]
    assert sorted(popped) == [("a", "v1"), ("a", "v3"), ("b", "v2")] # sorted as "b" is allowed to change order

    with pytest.raises(KeyError, match="HeaderMap is empty"):
        headers.popitem()
    with pytest.raises(KeyError, match="HeaderMap is empty"):
        HeaderMap().popitem()


@pytest.mark.parametrize(
    "pairs", [[], [("a", "v1")], [("a", "v1"), ("b", "v2")], [("a", "v1"), ("b", "v2"), ("a", "v3")]]
)
def test_clear(pairs: list[tuple[str, str]]):
    headers = HeaderMap(pairs)
    items = headers.items()
    keys = headers.keys()
    values = headers.values()
    assert len(headers) == len(pairs)
    assert len(items) == len(pairs) and len(keys) == len(pairs) and len(values) == len(pairs)
    headers.clear()
    assert len(headers) == 0
    assert len(items) == 0 and len(keys) == 0 and len(values) == 0


@pytest.mark.parametrize("kind", [list, tuple, dict, CIMultiDict, HeaderMap])
def test_update(kind: Callable[[list], Any]):
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    headers.update(kind([("c", "v3")]))
    assert len(headers) == 4
    assert headers["c"] == "v3"

    headers.update(a="v4")
    assert len(headers) == 3
    assert headers["a"] == "v4"

    headers.update({"a": "v5"}, c="v6")
    assert len(headers) == 3
    assert headers["a"] == "v5" and headers["c"] == "v6"

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers.update({"a\n": "v5"})
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers.update({"a": "v5\n"})

    assert len(headers) == 3
    assert sorted(headers.items()) == [("a", "v5"), ("b", "v2"), ("c", "v6")]


def test_setdefault():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    assert headers.setdefault("a", "v4") == "v1"
    assert len(headers) == 3
    assert headers.setdefault("c", "v4") == "v4"
    assert len(headers) == 4
    assert headers["c"] == "v4"

    with pytest.raises(TypeError, match="missing 1 required positional argument: 'default'"):
        headers.setdefault("a")
    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers.setdefault("a\n", "v5")
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers.setdefault("a", "v5\n")


def test_getall():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert headers.getall("a") == ["v1", "v3"]
    assert headers.getall("b") == ["v2"]
    assert headers.getall("c") == []
    assert headers.getall("a\n") == []


def test_insert():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    assert headers.insert("c", "v4") == []
    assert len(headers) == 4
    assert headers["c"] == "v4"

    assert headers.insert("a", "v5") == ["v1", "v3"]
    assert len(headers) == 3
    assert headers.getall("a") == ["v5"]

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers.insert("a\n", "v6")
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers.insert("a", "v6\n")

    assert len(headers) == 3
    assert sorted(headers.items()) == [("a", "v5"), ("b", "v2"), ("c", "v4")]


@pytest.mark.parametrize("sensitive_arg", [{}, {"is_sensitive": True}, {"is_sensitive": False}])
def test_insert__sensitive(sensitive_arg: dict):
    headers = HeaderMap([("a", "v1")])
    headers.insert("a", "v2", **sensitive_arg)
    assert [*headers.items()] == [("a", "v2")]

    if sensitive_arg.get("is_sensitive"):
        assert str(headers) == "{'a': 'Sensitive'}"
        assert repr(headers) == "HeaderMap({'a': 'Sensitive'})"
    else:
        assert str(headers) == "{'a': 'v2'}"
        assert repr(headers) == "HeaderMap({'a': 'v2'})"


def test_append():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    assert headers.append("c", "v4") is False
    assert len(headers) == 4
    assert headers["c"] == "v4"

    assert headers.append("a", "v5") is True
    assert len(headers) == 5
    assert headers.getall("a") == ["v1", "v3", "v5"]

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers.append("a\n", "v6")
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers.append("a", "v6\n")

    assert len(headers) == 5
    assert sorted(headers.items()) == [("a", "v1"), ("a", "v3"), ("a", "v5"), ("b", "v2"), ("c", "v4")]


@pytest.mark.parametrize("sensitive_arg", [{}, {"is_sensitive": True}, {"is_sensitive": False}])
def test_append__sensitive(sensitive_arg: dict):
    headers = HeaderMap([("a", "v1")])
    headers.append("a", "v2", **sensitive_arg)
    assert [*headers.items()] == [("a", "v1"), ("a", "v2")]

    if sensitive_arg.get("is_sensitive"):
        assert str(headers) == "{'a': ['v1', 'Sensitive']}"
        assert repr(headers) == "HeaderMap({'a': ['v1', 'Sensitive']})"
    else:
        assert str(headers) == "{'a': ['v1', 'v2']}"
        assert repr(headers) == "HeaderMap({'a': ['v1', 'v2']})"


@pytest.mark.parametrize("kind", [list, tuple, dict, CIMultiDict, HeaderMap])
def test_extend(kind: Callable[[list], Any]):
    headers = HeaderMap([("a", "v1"), ("b", "v2")])
    assert len(headers) == 2

    headers.extend(kind([("c", "v3"), ("a", "v4")]))
    assert len(headers) == 4
    assert headers.getall("c") == ["v3"]
    assert headers.getall("a") == ["v1", "v4"]

    with pytest.raises(ValueError, match="invalid HTTP header name"):
        headers.extend(kind([("a\n", "v1")]))
    with pytest.raises(ValueError, match="failed to parse header value"):
        headers.extend(kind([("a", "v1\n")]))

    assert len(headers) == 4
    assert sorted(headers.items()) == [("a", "v1"), ("a", "v4"), ("b", "v2"), ("c", "v3")]


def test_popall():
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert len(headers) == 3

    with pytest.raises(KeyError, match="'c'"):
        headers.popall("c")
    with pytest.raises(KeyError, match=re.escape("'a\\n'")):
        headers.popall("a\n")
    assert len(headers) == 3

    assert headers.popall("a") == ["v1", "v3"]
    assert len(headers) == 1
    assert headers.popall("b", []) == ["v2"]
    assert len(headers) == 0

    with pytest.raises(KeyError, match="'a'"):
        headers.popall("a")

    assert headers.popall("d", default=None) is None
    assert headers.popall("d", "default") == "default"
    assert headers.popall("d", []) == []


def test_dict_multi_value():
    headers = HeaderMap()
    assert type(headers.dict_multi_value()) == dict
    assert headers.dict_multi_value() == {}

    headers = HeaderMap([("a", "v1")])
    assert headers.dict_multi_value() == {"a": "v1"}

    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    assert headers.dict_multi_value() == {"a": ["v1", "v3"], "b": "v2"}
    headers.append("b", "v4", is_sensitive=True)
    assert headers.dict_multi_value() == {"a": ["v1", "v3"], "b": ["v2", "v4"]}


@pytest.mark.parametrize("std_copy", [False, True])
def test_copy(std_copy: bool):
    headers = HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])
    copied = copy(headers) if std_copy else headers.copy()
    assert copied == headers and copied is not headers
    assert copied.items() == headers.items() and copied.items() is not headers.items()
    assert copied.keys() == headers.keys() and copied.keys() is not headers.keys()
    assert copied.values() is not headers.values()
    copied_items = copied.items()
    items = headers.items()

    copied["c"] = "v4"
    assert len(copied) == 4 and len(copied_items) == 4
    assert len(headers) == 3 and len(items) == 3
    assert copied != headers
    assert copied.items() != headers.items()
    assert copied.keys() != headers.keys()
    assert copied.getall("c") == ["v4"]
    assert headers.getall("c") == []


def test_str():
    assert str(HeaderMap()) == "{}"
    assert str(HeaderMap([("a", "v1")])) == "{'a': 'v1'}"
    assert str(HeaderMap([("a", "v1"), ("b", "v2")])) == "{'a': 'v1', 'b': 'v2'}"
    assert str(HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])) == "{'a': ['v1', 'v3'], 'b': 'v2'}"


def test_repr():
    assert repr(HeaderMap()) == "HeaderMap({})"
    assert repr(HeaderMap([("a", "v1")])) == "HeaderMap({'a': 'v1'})"
    assert repr(HeaderMap([("a", "v1"), ("b", "v2")])) == "HeaderMap({'a': 'v1', 'b': 'v2'})"
    assert repr(HeaderMap([("a", "v1"), ("b", "v2"), ("a", "v3")])) == "HeaderMap({'a': ['v1', 'v3'], 'b': 'v2'})"


def test_abc():
    headers = HeaderMap()
    items = headers.items()
    keys = headers.keys()
    values = headers.values()
    assert type(headers) == HeaderMap
    assert isinstance(headers, MutableMapping) and isinstance(headers, Mapping)
    assert not isinstance(headers, dict)

    assert issubclass(HeaderMap, MutableMapping) and issubclass(HeaderMap, Mapping)
    assert not issubclass(HeaderMap, dict)

    assert type(items) == HeaderMapItemsView
    assert isinstance(items, ItemsView)
    assert not isinstance(items, type(dict().items()))
    assert type(keys) == HeaderMapKeysView
    assert isinstance(keys, KeysView)
    assert not isinstance(keys, type(dict().keys()))
    assert type(values) == HeaderMapValuesView
    assert isinstance(values, ValuesView)
    assert not isinstance(values, type(dict().values()))

    assert issubclass(HeaderMapItemsView, ItemsView)
    assert not issubclass(HeaderMapItemsView, type(dict().items()))
    assert issubclass(HeaderMapKeysView, KeysView)
    assert not issubclass(HeaderMapKeysView, type(dict().keys()))
    assert issubclass(HeaderMapValuesView, ValuesView)
    assert not issubclass(HeaderMapValuesView, type(dict().values()))
