"""Orders module: functions, async, classes, methods, properties."""

import decimal
import typing
from decimal import Decimal
from typing import Optional as Maybe

TAX_RATE: Decimal = Decimal("0.20")


def total(items: list["Item"], tax: bool = True) -> Decimal:
    """Sum item prices."""
    amount = sum((item.price for item in items), Decimal("0"))
    return amount * (1 + TAX_RATE) if tax else amount


async def fetch_order(order_id: int) -> "Order":
    """Load one order asynchronously."""
    return Order(order_id, [])


class Item:
    """One purchasable item."""

    kind: str = "generic"

    def __init__(self, name: str, price: Decimal) -> None:
        self.name = name
        self.price = price

    @property
    def label(self) -> str:
        return f"{self.name} ({self.kind})"

    @staticmethod
    def parse(raw: str) -> "Item":
        return Item(raw, Decimal("0"))

    @classmethod
    def bulk(cls, names: list[str]) -> list["Item"]:
        return [cls.parse(name) for name in names]


class Order:
    """One placed order."""

    def __init__(self, order_id: int, items: list[Item]) -> None:
        self.order_id = order_id
        self.items = items

    async def refresh(self) -> None:
        pass


def _rename_guard() -> None:
    def inner_local() -> None:
        pass

    inner_local()
