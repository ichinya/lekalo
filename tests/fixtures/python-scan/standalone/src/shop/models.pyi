"""Declaration-only stub for the scanner fixture."""

def total(items: list, tax: bool = ...) -> object: ...

class Item:
    kind: str
    @property
    def label(self) -> str: ...
