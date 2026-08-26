"""one-shot check helper — re-exported from facade for `from deko_guard.core.check import check`."""
from deko_guard.client.facade import Deko
_default = None

def check(intent: str, **kwargs):
    global _default
    if _default is None:
        _default = Deko()
    return _default.check(intent, **kwargs)
