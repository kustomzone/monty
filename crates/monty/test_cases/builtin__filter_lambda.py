# xfail=cpython
# filter() with lambda (a closure)
# This should error until user-defined functions are supported
filter(lambda x: x > 0, [1, 2])

"""
TRACEBACK:
Traceback (most recent call last):
  File "builtin__filter_lambda.py", line 4, in <module>
    filter(lambda x: x > 0, [1, 2])
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
TypeError: filter() predicate must be None or a builtin function (user-defined functions not yet supported)
"""
