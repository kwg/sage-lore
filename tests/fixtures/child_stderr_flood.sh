#!/bin/sh
dd if=/dev/zero bs=1024 count=1024 2>/dev/null | tr '\0' 'x' >&2
echo "stdout after flood"
