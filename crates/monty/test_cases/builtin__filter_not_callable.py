# xfail=cpython
# filter() with non-callable first argument
# CPython's filter is lazy so it doesn't error until iteration
filter(4, [1, 2])

"""
TRACEBACK:
Traceback (most recent call last):
  File "builtin__filter_not_callable.py", line 4, in <module>
    filter(4, [1, 2])
    ~~~~~~~~~~~~~~~~~
TypeError: 'int' object is not callable
"""
