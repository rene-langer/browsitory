### Syntax highlighting

- Changed: C++ files (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh`) are highlighted with the C++
  grammar again, so templates, namespaces, `::` and raw strings look right. GLSL inside
  `R"glsl(...)glsl"` raw strings uses a small grammar of Browsitory's own, in place of one that
  has no licence. `.h` files stay C (#326).
