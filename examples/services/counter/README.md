# Counter Service

The canonical payload is one little-endian signed 64-bit increment. Refine
returns the same eight bytes. Accumulate reads the first successful result,
adds it to the `counter` key using saturation, stores little-endian `i64`, and
yields.
