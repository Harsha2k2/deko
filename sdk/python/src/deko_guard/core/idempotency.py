"""deterministic idempotency key derivation."""
import hashlib
import json

def derive_key(qualname: str, args: tuple, kwargs: dict) -> str:
    payload = json.dumps({"fn": qualname, "args": args, "kwargs": sorted(kwargs.items())}, sort_keys=True, default=str)
    return hashlib.sha256(payload.encode()).hexdigest()[:32]
