"""Dynamic-construct fixture: every row must surface as uncertainty."""

import importlib
import os

if os.environ.get("SHOP_EXTRA"):
    import json  # conditional-import

try:
    import optional_dep
except ImportError:
    optional_dep = None


def load(name: str):
    module = importlib.import_module(name)
    return eval("module.Attribute")


def patched():
    os.environ["X"] = "1"
    os.get_terminal_size = lambda: (80, 24)  # monkey-patch candidate


GLOBAL_FLAG = True


def flip():
    global GLOBAL_FLAG
    GLOBAL_FLAG = False
    exec("x = 1")
