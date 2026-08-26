"""deko-guard — control plane for ai agent actions."""

__version__ = "2.0.0a0"

from deko_guard.core.errors import DekoDeniedError, DekoEscalatedError, DekoError
from deko_guard.core.types import Verdict, ForwardResult, GuardOptions

try:
    from deko_guard.client.facade import Deko
except ImportError:
    Deko = None  # type: ignore

__all__ = ["Deko", "DekoDeniedError", "DekoEscalatedError", "DekoError", "Verdict", "ForwardResult", "GuardOptions", "__version__"]
