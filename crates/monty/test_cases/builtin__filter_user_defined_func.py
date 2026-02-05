# xfail=cpython
# filter() with user-defined function
# This should error until user-defined functions are supported
def is_positive(x):
    return x > 0


filter(is_positive, [1, 2])

"""
TRACEBACK:
Traceback (most recent call last):
  File "builtin__filter_user_defined_func.py", line 8, in <module>
    filter(is_positive, [1, 2])
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~
TypeError: filter() predicate must be None or a builtin function (user-defined functions not yet supported)
"""
