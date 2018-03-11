#!/bin/sh

cat > build.py << EOF
from distutils.core import setup, Extension

native_ext = Extension('llbridge',
    include_dirs = ['..'],
    libraries = ['lightning_bridge'],
    sources = ['llbridge_wrap.cxx']
)

setup(name = 'llbridge',
    version = '0.1.0',
    description = 'llbridge',
    author = 'Heyang Zhou',
    author_email = 'i@ifxor.com',
    url = 'https://github.com/losfair/liblightning',
    long_description = 'llbridge',
    ext_modules = [native_ext]
)
EOF
python3 build.py build || exit 1
cp build/lib.*/*.so _llbridge.so || exit 1

cat > test.py << EOF
import llbridge

class CoroutineEntry(llbridge.CoroutineEntryCallback):
    def __init__(self):
        super().__init__()

    def call(self):
        pass

sched = llbridge.ll_scheduler_new()
entry = CoroutineEntry()
llbridge.ll_hl_scheduler_start_coroutine(sched, entry)
n_runs = llbridge.ll_scheduler_run_once(sched, 20)
assert n_runs == 1
n_runs = llbridge.ll_scheduler_run_once(sched, 20)
assert n_runs == 0
llbridge.ll_scheduler_destroy(sched)
EOF

python3 test.py || exit 1
echo "Tests for python bindings have passed."
