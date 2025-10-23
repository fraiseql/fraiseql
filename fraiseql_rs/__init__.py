# Try to import from installed version first
try:
    import fraiseql_rs as _installed

    if hasattr(_installed, "build_graphql_response"):
        # Installed version has functions, use it
        from fraiseql_rs import *
    else:
        raise ImportError("Installed version missing functions")
except ImportError:
    # Load directly from .so file
    import importlib.util
    import os

    so_file = os.path.join(
        os.path.dirname(os.path.dirname(__file__)),
        ".venv",
        "lib",
        "python3.13",
        "site-packages",
        "fraiseql_rs",
        "fraiseql_rs.cpython-313-x86_64-linux-gnu.so",
    )
    if os.path.exists(so_file):
        spec = importlib.util.spec_from_file_location("fraiseql_rs", so_file)
        _ext_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(_ext_module)
        # Copy to this module
        import sys

        this_module = sys.modules[__name__]
        for name in dir(_ext_module):
            if not name.startswith("_"):
                setattr(this_module, name, getattr(_ext_module, name))
