# xfail=cpython
# nonlocal at module level is a syntax error
nonlocal x  # type: ignore
"""
TRACEBACK:
Traceback (most recent call last):
  File "traceback__nonlocal_module_scope.py", line 3, in <module>
    nonlocal x  # type: ignore
    ~~~~~~~~~~
SyntaxError: nonlocal declaration not allowed at module level
"""
