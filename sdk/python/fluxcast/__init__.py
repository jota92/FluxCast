"""FluxCast FCDP v0.1 framing SDK (Python 3.9+, standard library only)."""
from .fcdp import FcdpHeader, decode, encode

__all__ = ["FcdpHeader", "decode", "encode"]
