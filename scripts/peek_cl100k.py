#!/usr/bin/env python3
"""
Peek the first 20 and last 10 decoded mapping lines from cl100k_base_data.h

This script parses the C header's byte array, decodes it into ASCII text
containing lines of the form:
  <base64_token> <id>

It then prints the first 20 and last 10 lines to STDOUT.
"""
from __future__ import annotations
import os
import re
import sys


def extract_bytes_from_header(header_path: str) -> bytes:
    with open(header_path, 'rb') as f:
        content = f.read().decode('latin1')

    # Find the array initializer block
    m = re.search(r"cl100k_base_tiktoken\[\]\s*=\s*\{(.*?)\}\s*;", content, re.S)
    if not m:
        raise RuntimeError("Could not find cl100k_base_tiktoken array in header")
    body = m.group(1)

    # Extract hex byte tokens like 0x49, 0x51, ...
    hex_bytes = re.findall(r"0x([0-9a-fA-F]{2})", body)
    data = bytes(int(h, 16) for h in hex_bytes)
    return data


def main() -> None:
    if len(sys.argv) < 2:
        here = os.path.dirname(os.path.abspath(__file__))
        default_path = os.path.join(here, '..', 'integration-work', 'fast-pdf-parser', 'include', 'fast_pdf_parser', 'cl100k_base_data.h')
        header_path = os.path.abspath(default_path)
    else:
        header_path = sys.argv[1]

    if not os.path.exists(header_path):
        print(f"Header not found: {header_path}", file=sys.stderr)
        sys.exit(1)

    raw = extract_bytes_from_header(header_path)
    text = raw.decode('utf-8', errors='replace')
    lines = [ln for ln in text.splitlines() if ln.strip()]

    print("Decoded mapping preview:")
    print("-- first 20 lines --")
    for ln in lines[:20]:
        print(ln)
    print("-- last 10 lines --")
    for ln in lines[-10:]:
        print(ln)


if __name__ == '__main__':
    main()

