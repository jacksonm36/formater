Windows x86_64 release binaries ( MSVC ), copied from cargo build --release.

  formater.exe      — parancssor
  formater-gui.exe  — asztali felület

A forráskód és a teljes projekt a repo gyökérben van. Újabb build: zárd be a formater-gui.exe-t, majd a repo gyökérből:
  cargo build --release
és másold ide a target\release\*.exe fájlokat.
