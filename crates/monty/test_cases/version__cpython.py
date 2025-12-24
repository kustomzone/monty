# xfail=monty
import sys

assert sys.version_info[:2] == (3, 13), f'Expected Python 3.13, got {sys.version_info[:2]}'
