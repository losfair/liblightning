#!/bin/sh

rm -r bindings || true
mkdir bindings

cbindgen -l c > bindings/llbridge.h
cbindgen -l c++ > bindings/llbridge.hpp
cp llbridge_highlevel.hpp bindings/

cd bindings

mkdir python
cd python
swig -c++ -python -outcurrentdir ../../llbridge.i || exit 1
sh ../../bindgen_scripts/python.sh || exit 1
cd ..

mkdir javascript
cd javascript
swig -c++ -javascript -node -outcurrentdir ../../llbridge.i || exit 1
cd ..

echo "All bindings generated successfully".
