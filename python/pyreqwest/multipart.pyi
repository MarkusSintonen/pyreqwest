# src/multipart/form.pyi
from typing_extensions import Self


class Form:
    def __init__(self) -> None: ...
    def text(self, name: str, value: str) -> Self: ...
